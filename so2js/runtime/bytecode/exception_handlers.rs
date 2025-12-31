use super::{
    generator::GenRegister,
    operand::{min_width_for_unsigned, Operand, Register},
    width::{ExtraWide, WidthEnum},
};
use crate::{
    field_offset,
    runtime::{
        alloc_error::AllocResult,
        collections::InlineArray,
        debug_print::{DebugPrint, DebugPrinter},
        gc::{GcVisitorExt, HeapItem},
        heap_item_descriptor::{HeapItemDescriptor, HeapItemKind},
        Context, HeapPtr, StackRoot,
    },
    set_uninit,
};
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

pub struct ExceptionStackRootrBuilder {
    /// Byte offset of the start of the instruction range that is covered (inclusive).
    pub start: usize,
    /// Byte offset of the end of the instruction range that is covered (exclusive).
    pub end: usize,
    /// Byte offset of the handler block that is run when an exception in this range occurs.
    pub handler: usize,
    /// Register in which to the place the error value.
    pub error_register: Option<GenRegister>,
}

impl ExceptionStackRootrBuilder {
    /// Create a new exception handler with start and end offsets. The handler offset and register
    /// index will be filled in later.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            handler: 0,
            error_register: None,
        }
    }
}

pub struct ExceptionStackRootrsBuilder {
    /// Collection fo all handlers generated so far in the function.
    handlers: Vec<ExceptionStackRootrBuilder>,
    /// The minimum width that fits all numbers in the handlers generated so far.
    width: WidthEnum,
}

impl ExceptionStackRootrsBuilder {
    pub fn new() -> Self {
        Self {
            handlers: vec![],
            width: WidthEnum::Narrow,
        }
    }

    pub fn add(&mut self, handler: ExceptionStackRootrBuilder) {
        // Determine the min width that fits all numbers in the handler
        self.width = self
            .width
            .max(min_width_for_unsigned(handler.start))
            .max(min_width_for_unsigned(handler.end))
            .max(min_width_for_unsigned(handler.handler));

        if let Some(reg) = handler.error_register {
            self.width = self.width.max(reg.min_width());
        }

        self.handlers.push(handler);
    }

    pub fn finish(&self, cx: Context) -> AllocResult<Option<StackRoot<ExceptionStackRootrs>>> {
        if self.handlers.is_empty() {
            return Ok(None);
        }

        let mut buffer = vec![];
        for handler in &self.handlers {
            self.write_operand(&mut buffer, handler.start);
            self.write_operand(&mut buffer, handler.end);
            self.write_operand(&mut buffer, handler.handler);

            // The `this` register is used as a sigil value to represent a missing register
            let register = handler.error_register.unwrap_or(Register::this());
            self.write_operand(&mut buffer, register.signed() as isize as usize);
        }

        Ok(Some(ExceptionStackRootrs::new(cx, buffer, self.width)?))
    }

    fn write_operand(&self, buffer: &mut Vec<u8>, value: usize) {
        match self.width {
            WidthEnum::Narrow => {
                buffer.push(value as u8);
            }
            WidthEnum::Wide => {
                buffer.extend_from_slice(&u16::to_ne_bytes(value as u16));
            }
            WidthEnum::ExtraWide => {
                buffer.extend_from_slice(&usize::to_ne_bytes(value));
            }
        }
    }
}

#[repr(C)]
pub struct ExceptionStackRootrs {
    descriptor: HeapPtr<HeapItemDescriptor>,
    /// Width of the encoded handler data. A narrow or wide width means all values are encoded as
    /// one or two bytes, respectively. An extra wide width means all values are encoded as a full
    /// eight bytes.
    width: WidthEnum,
    /// Encoded handlers data.
    handlers: InlineArray<u8>,
}

impl ExceptionStackRootrs {
    fn new(
        cx: Context,
        handlers: Vec<u8>,
        width: WidthEnum,
    ) -> AllocResult<StackRoot<ExceptionStackRootrs>> {
        let size = Self::calculate_size_in_bytes(handlers.len());
        let mut object = cx.alloc_uninit_with_size::<ExceptionStackRootrs>(size)?;

        set_uninit!(
            object.descriptor,
            cx.base_descriptors.get(HeapItemKind::ExceptionStackRootrs)
        );
        set_uninit!(object.width, width);
        object.handlers.init_from_slice(&handlers);

        Ok(object.to_stack(cx))
    }

    const HANDLERS_BYTE_OFFSET: usize = field_offset!(ExceptionStackRootrs, handlers);

    fn calculate_size_in_bytes(handlers_len: usize) -> usize {
        Self::HANDLERS_BYTE_OFFSET + InlineArray::<u8>::calculate_size_in_bytes(handlers_len)
    }

    /// A zero-copy GC-unsafe iterator over the exception handlers.
    pub fn iter(&self) -> ExceptionStackRootrsIterator {
        let range = self.handlers.as_slice().as_ptr_range();
        ExceptionStackRootrsIterator {
            current: range.start,
            end: range.end,
            width: self.width,
        }
    }
}

/// A zero-copy GC-unsafe iterator over the exception handlers.
pub struct ExceptionStackRootrsIterator {
    current: *const u8,
    end: *const u8,
    width: WidthEnum,
}

/// A view of an exception handler entry in the exception handler table.
#[derive(Clone, Copy)]
pub struct ExceptionStackRootr {
    /// Pointer to the start of the exception handler entry.
    ptr: *const u8,
    /// Byte width of the values in this entry.
    width: WidthEnum,
}

impl ExceptionStackRootr {
    fn get_value_at(&self, index: usize) -> usize {
        unsafe {
            match self.width {
                WidthEnum::Narrow => *self.ptr.add(index) as usize,
                WidthEnum::Wide => *self.ptr.add(index * 2).cast::<u16>() as usize,
                WidthEnum::ExtraWide => *self.ptr.add(index * 8).cast::<usize>(),
            }
        }
    }

    pub fn start(&self) -> usize {
        self.get_value_at(0)
    }

    pub fn end(&self) -> usize {
        self.get_value_at(1)
    }

    pub fn handler(&self) -> usize {
        self.get_value_at(2)
    }

    pub fn error_register(&self) -> Option<Register<ExtraWide>> {
        let raw_value = unsafe {
            match self.width {
                WidthEnum::Narrow => *self.ptr.add(3).cast::<i8>() as isize,
                WidthEnum::Wide => *self.ptr.add(6).cast::<i16>() as isize,
                WidthEnum::ExtraWide => *self.ptr.add(24).cast::<isize>(),
            }
        };

        // The `this` register is a sigil for no register
        let register = Register::from_signed(raw_value as i32);
        if register.is_this() {
            None
        } else {
            Some(register)
        }
    }
}

impl Iterator for ExceptionStackRootrsIterator {
    type Item = ExceptionStackRootr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.end {
            None
        } else {
            let view = ExceptionStackRootr {
                ptr: self.current,
                width: self.width,
            };

            let entry_size = match self.width {
                WidthEnum::Narrow => 1,
                WidthEnum::Wide => 2,
                WidthEnum::ExtraWide => 4,
            };
            self.current = unsafe { self.current.add(entry_size * 4) };

            Some(view)
        }
    }
}

impl DebugPrint for HeapPtr<ExceptionStackRootrs> {
    fn debug_format(&self, printer: &mut DebugPrinter) {
        if printer.is_short_mode() {
            printer.write_heap_item_default(self.cast());
            return;
        }

        // Exception handlers are indented
        printer.write("Exception StackRootrs:\n");
        printer.inc_indent();

        for handler in self.iter() {
            printer.write_indent();
            printer.write(&format!(
                "{}-{} -> {}",
                handler.start(),
                handler.end(),
                handler.handler()
            ));

            if let Some(register) = handler.error_register() {
                printer.write(&format!(" ({register})"));
            }

            printer.write("\n");
        }

        printer.dec_indent();
    }
}

impl HeapItem for HeapPtr<ExceptionStackRootrs> {
    fn byte_size(&self) -> usize {
        ExceptionStackRootrs::calculate_size_in_bytes(self.handlers.len())
    }

    fn visit_pointers(&mut self, visitor: &mut impl GcVisitorExt) {
        visitor.visit_pointer(&mut self.descriptor);
    }
}
