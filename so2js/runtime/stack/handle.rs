use crate::runtime::{
    context::ContextCell,
    object_value::ObjectValue,
    string_value::StringValue,
    value::{BigIntValue, SymbolValue},
    Context, Value,
};
use alloc::boxed::Box;
use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::Pin,
    ptr::NonNull,
};

use crate::runtime::gc::{GcVisitorExt, HeapPtr, IsHeapItem};

/// StackRoots store a pointer-sized unit of data. This may be either a value or a heap pointer.
pub type StackRootContents = usize;

pub trait ToStackRootContents {
    type Impl;

    fn to_handle_contents(value: Self::Impl) -> StackRootContents;
}

/// StackRoots hold a value or heap pointer behind a pointer. StackRoots are safe to store on the stack
/// during a GC, since the handle's pointer does not change but the address of the heap item
/// behind the pointer may be updated. All handle creation must be given an explicit handle
/// context (no implicit Context lookup).
pub struct StackRoot<T> {
    ptr: NonNull<StackRootContents>,
    phantom_data: PhantomData<T>,
}

impl core::fmt::Debug for StackRoot<Value> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_dangling() {
            write!(f, "StackRoot(dangling)")
        } else {
            let value = self.deref();
            write!(f, "StackRoot({:?})", value)
        }
    }
}

impl<T: ToStackRootContents> StackRoot<T> {
    #[inline]
    pub fn new(handle_context: &mut StackRootContext, contents: StackRootContents) -> StackRoot<T> {
        // StackRoot scope block is full, so push a new handle scope block onto stack
        if handle_context.next_ptr == handle_context.end_ptr {
            handle_context.push_block();
        }

        // Write pointer into handle's address
        let handle = handle_context.next_ptr;
        unsafe { handle.write(contents) };

        handle_context.next_ptr = unsafe { handle.add(1) };

        // Increment handle count if tracking handles
        #[cfg(feature = "handle_stats")]
        {
            handle_context.num_handles += 1;
            handle_context.max_handles = handle_context.max_handles.max(handle_context.num_handles);
        }

        StackRoot {
            ptr: unsafe { NonNull::new_unchecked(handle.cast()) },
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn empty(mut cx: Context) -> StackRoot<T> {
        let handle_context = &mut cx.handle_context;
        StackRoot::new(handle_context, Value::to_handle_contents(Value::empty()))
    }

    #[inline]
    pub const fn dangling() -> StackRoot<T> {
        StackRoot {
            ptr: NonNull::dangling(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn is_dangling(&self) -> bool {
        self.ptr == NonNull::dangling()
    }

    /// Replace the value stored behind this handle with a new value. Note that all copies of this
    /// handle will also be changed.
    #[inline]
    pub fn replace(&mut self, new_contents: T::Impl) {
        unsafe { self.ptr.as_ptr().write(T::to_handle_contents(new_contents)) }
    }

    pub fn replace_into<U: ToStackRootContents>(self, new_contents: U::Impl) -> StackRoot<U> {
        let mut handle = self.cast::<U>();
        handle.replace(new_contents);
        handle
    }
}

impl<T> StackRoot<T> {
    #[inline]
    pub fn cast<U>(&self) -> StackRoot<U> {
        StackRoot {
            ptr: self.ptr,
            phantom_data: PhantomData,
        }
    }
}

impl<T> Clone for StackRoot<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for StackRoot<T> {}

impl<T: ToStackRootContents> Deref for StackRoot<T> {
    type Target = T::Impl;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.cast::<Self::Target>().as_ref() }
    }
}

impl<T: ToStackRootContents> DerefMut for StackRoot<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.cast::<Self::Target>().as_mut() }
    }
}

/// Saved handle state that allows restoring to the state right before a handle scope was entered.
/// Must only be created on the stack.
#[must_use = "StackRootScopes must be explicitly exited with a call to exit"]
pub struct StackRootScope {
    context_ptr: *mut ContextCell,
    next_ptr: *mut StackRootContents,
    end_ptr: *mut StackRootContents,
}

impl StackRootScope {
    #[inline]
    pub fn new<F: FnOnce(Context) -> R, R: Escapable>(cx: Context, f: F) -> R {
        let stack_scope = Self::enter(cx);
        let result = f(cx);
        stack_scope.escape(cx, result)
    }

    #[inline]
    pub fn enter(mut cx: Context) -> StackRootScope {
        let context_ptr = cx.as_ptr();
        let handle_context = &mut cx.handle_context;
        let next_ptr = handle_context.next_ptr;
        let end_ptr = handle_context.end_ptr;

        StackRootScope {
            context_ptr,
            next_ptr,
            end_ptr,
        }
    }

    /// Exit a handle scope and return an item escaped into the parent's handle scope.
    #[inline]
    pub fn escape<R: Escapable>(self, cx: Context, result: R) -> R {
        self.exit();
        result.escape(cx)
    }

    /// Exit a handle scope without returning an escaped item.
    #[inline]
    pub fn exit(self) {
        self.exit_non_consuming();
    }

    #[inline]
    fn exit_non_consuming(&self) {
        let context_cell = unsafe { &mut *self.context_ptr };
        let handle_context = &mut context_cell.handle_context;

        // The saved handle scope was in a previous block. Pop blocks until the current block
        // matches that of the saved handle scope.
        if self.end_ptr != handle_context.end_ptr {
            // If tracking handles then decrement the handle count for the first popped block. This
            // removes the handle range from the start of the block to the next pointer.
            #[cfg(feature = "handle_stats")]
            {
                let unallocated_in_block =
                    unsafe { handle_context.end_ptr.offset_from(handle_context.next_ptr) as usize };
                handle_context.num_handles -= HANDLE_BLOCK_SIZE - unallocated_in_block;
            }

            while self.end_ptr != handle_context.pop_block() {
                // All later blocks were fully allocated
                #[cfg(feature = "handle_stats")]
                {
                    handle_context.num_handles -= HANDLE_BLOCK_SIZE;
                }
            }

            // Decrement the handle count for newly deallocated handles in the new current block.
            // These handles are from the next pointer to the end of the block.
            #[cfg(feature = "handle_stats")]
            {
                handle_context.num_handles -=
                    unsafe { self.end_ptr.offset_from(self.next_ptr) } as usize;
            }
        } else {
            // If tracking handles then remove the handle range in this block that was deallocated.
            #[cfg(feature = "handle_stats")]
            {
                handle_context.num_handles -=
                    unsafe { handle_context.next_ptr.offset_from(self.next_ptr) } as usize;
            }
        }

        handle_context.next_ptr = self.next_ptr;
        handle_context.end_ptr = self.end_ptr;
    }
}

/// A guard which enters a handle scope and exits it when dropped. Does not escape any values.
pub struct StackRootScopeGuard {
    stack_scope: StackRootScope,
}

impl StackRootScopeGuard {
    #[inline]
    pub fn new(cx: Context) -> StackRootScopeGuard {
        StackRootScopeGuard {
            stack_scope: StackRootScope::enter(cx),
        }
    }
}

impl Drop for StackRootScopeGuard {
    #[inline]
    fn drop(&mut self) {
        self.stack_scope.exit_non_consuming();
    }
}

/// A guard which enters a handle scope and exits it when dropped. Does not escape any values.
#[macro_export]
macro_rules! js_stack_scope_guard {
    ($cx:expr) => {
        let _guard = $crate::runtime::stack::StackRootScopeGuard::new($cx);
    };
}

/// Enter a handle scope and execute the given statement. Returns and escapes the result of
/// executing the statement.
#[macro_export]
macro_rules! js_stack_scope {
    ($cx:expr, $body:stmt) => {
        $crate::runtime::stack::StackRootScope::new($cx, |_| {
            let result = { $body };
            result
        })
    };
}

/// Number of handles contained in a single handle block. Default to 4KB handle blocks.
const HANDLE_BLOCK_SIZE: usize = 512;

pub struct StackRootBlock {
    ptrs: [StackRootContents; HANDLE_BLOCK_SIZE],
    // Pointer to the start of the handles array
    start_ptr: *mut StackRootContents,
    // Pointer to the end of the handles array. Used to uniquely identify this block.
    end_ptr: *mut StackRootContents,
    prev_block: Option<Pin<Box<StackRootBlock>>>,
}

impl StackRootBlock {
    fn new(prev_block: Option<Pin<Box<StackRootBlock>>>) -> Pin<Box<StackRootBlock>> {
        // Block must first be allocated on heap before start and end ptrs can be calculated.
        let mut block = Pin::new(Box::new(StackRootBlock {
            ptrs: [0; HANDLE_BLOCK_SIZE],
            start_ptr: core::ptr::null_mut(),
            end_ptr: core::ptr::null_mut(),
            prev_block,
        }));

        let range = block.ptrs.as_mut_ptr_range();
        block.start_ptr = range.start;
        block.end_ptr = range.end;

        block
    }
}

pub struct StackRootContext {
    /// Pointer to within a handle block, pointing to address of the next handle to allocate
    next_ptr: *mut StackRootContents,

    /// Pointer one beyond the end of the current handle scope block, marking the limit for this
    /// handle scope. Used to uniquely identify the current handle block.
    end_ptr: *mut StackRootContents,

    /// Current block for the handle scope stack. Contains chain of other blocks in use.
    current_block: Pin<Box<StackRootBlock>>,

    /// Chain of free blocks
    free_blocks: Option<Pin<Box<StackRootBlock>>>,

    /// Total number of handles currently allocated
    #[cfg(feature = "handle_stats")]
    num_handles: usize,

    /// Max number of handles allocated at once observed so far
    #[cfg(feature = "handle_stats")]
    max_handles: usize,
}

#[cfg(feature = "handle_stats")]
#[derive(Debug)]
pub struct StackRootStats {
    pub num_handles: usize,
    pub max_handles: usize,
}

impl StackRootContext {
    /// Create a new StackRootContext with its first block allocated
    pub fn new() -> StackRootContext {
        let first_block = StackRootBlock::new(None);

        StackRootContext {
            next_ptr: first_block.start_ptr,
            end_ptr: first_block.end_ptr,
            current_block: first_block,
            free_blocks: None,
            #[cfg(feature = "handle_stats")]
            num_handles: 0,
            #[cfg(feature = "handle_stats")]
            max_handles: 0,
        }
    }

    pub fn init(&mut self) {
        let first_block = StackRootBlock::new(None);

        let handle_context = StackRootContext {
            next_ptr: first_block.start_ptr,
            end_ptr: first_block.end_ptr,
            current_block: first_block,
            free_blocks: None,
            #[cfg(feature = "handle_stats")]
            num_handles: 0,
            #[cfg(feature = "handle_stats")]
            max_handles: 0,
        };

        // Initial value was uninitialized, so replace without dropping uninitialized value
        core::mem::forget(core::mem::replace(self, handle_context));
    }

    fn push_block(&mut self) {
        match &mut self.free_blocks {
            None => {
                // Allocate a new block and push it as the current block
                let new_block = StackRootBlock::new(None);
                let old_current_block = core::mem::replace(&mut self.current_block, new_block);
                self.current_block.prev_block = Some(old_current_block);
            }
            Some(free_blocks) => {
                // Pull the top free block off of the free list
                let rest_free_blocks = free_blocks.prev_block.take();
                let free_block = core::mem::replace(&mut self.free_blocks, rest_free_blocks);

                // Push free block as the current block
                let old_current_block =
                    core::mem::replace(&mut self.current_block, free_block.unwrap());
                self.current_block.prev_block = Some(old_current_block);
            }
        }

        self.next_ptr = self.current_block.start_ptr;
        self.end_ptr = self.current_block.end_ptr;
    }

    fn pop_block(&mut self) -> *mut StackRootContents {
        // Current block is replaced by its previous block
        let old_prev_block = self.current_block.prev_block.take();

        let new_current_block = old_prev_block.unwrap();
        let new_end_ptr = new_current_block.end_ptr;
        let old_current_block = core::mem::replace(&mut self.current_block, new_current_block);

        // Current block is moved to start of free list
        let old_free_blocks = self.free_blocks.replace(old_current_block);
        if let Some(new_first_free_block) = &mut self.free_blocks {
            new_first_free_block.prev_block = old_free_blocks;
        }

        // Return the end pointer for the new current block, uniquely identifying the new current block
        new_end_ptr
    }

    /// Return the number of handles that are currently being used.
    ///
    /// Currently only used for debugging.
    #[allow(dead_code)]
    pub fn handle_count(&self) -> usize {
        // Number of handles used in the current block
        let mut total =
            unsafe { HANDLE_BLOCK_SIZE - (self.end_ptr.offset_from(self.next_ptr) as usize) };

        // Add handles used in previous handle blocks
        let mut current_block = &self.current_block;
        while let Some(next_block) = &current_block.prev_block {
            current_block = next_block;
            total += HANDLE_BLOCK_SIZE;
        }

        total
    }

    /// Return the number of free handle blocks in the free list.
    ///
    /// Currently only used for debugging.
    #[allow(dead_code)]
    pub fn free_handle_block_count(&self) -> usize {
        let mut total = 0;

        let mut current_block = &self.free_blocks;
        while let Some(next_block) = current_block {
            current_block = &next_block.prev_block;
            total += 1;
        }

        total
    }

    pub fn visit_roots(&mut self, visitor: &mut impl GcVisitorExt) {
        // Only visit values that have been used (aka before the next pointer) in the current block
        let mut current_block = &self.current_block;
        Self::visit_roots_between_pointers(current_block.start_ptr, self.next_ptr, visitor);

        // Visit all values in earlier blocks
        while let Some(prev_block) = &current_block.prev_block {
            current_block = prev_block;
            Self::visit_roots_between_pointers(
                current_block.start_ptr,
                current_block.end_ptr,
                visitor,
            );
        }
    }

    fn visit_roots_between_pointers(
        start_ptr: *const StackRootContents,
        end_ptr: *const StackRootContents,
        visitor: &mut impl GcVisitorExt,
    ) {
        unsafe {
            let mut current_ptr = start_ptr;
            while current_ptr != end_ptr {
                let value_ref = &mut *(current_ptr.cast_mut() as *mut Value);
                visitor.visit_value(value_ref);

                current_ptr = current_ptr.add(1)
            }
        }
    }

    #[cfg(feature = "handle_stats")]
    pub fn handle_stats(&self) -> StackRootStats {
        StackRootStats {
            num_handles: self.num_handles,
            max_handles: self.max_handles,
        }
    }
}

impl StackRoot<Value> {
    #[inline]
    pub fn from_fixed_non_heap_ptr(value_ref: &Value) -> StackRoot<Value> {
        let ptr = unsafe { NonNull::new_unchecked(value_ref as *const Value as *mut Value) };
        StackRoot {
            ptr: ptr.cast(),
            phantom_data: PhantomData,
        }
    }

    #[inline]
    pub fn as_object(&self) -> StackRoot<ObjectValue> {
        self.cast()
    }

    #[inline]
    pub fn as_string(&self) -> StackRoot<StringValue> {
        self.cast()
    }

    #[inline]
    pub fn as_symbol(&self) -> StackRoot<SymbolValue> {
        self.cast()
    }

    #[inline]
    pub fn as_bigint(&self) -> StackRoot<BigIntValue> {
        self.cast()
    }
}

impl Value {
    /// Root a Value (pointer or immediate) using an explicit Context/handle context.
    /// Does not attempt to recover Context from GC headers; callers must pass the Context they own.
    #[inline]
    pub fn to_stack(self, mut cx: Context) -> StackRoot<Value> {
        let handle_context = &mut cx.handle_context;
        StackRoot::new(handle_context, Value::to_handle_contents(self))
    }
}

impl<T: IsHeapItem> HeapPtr<T> {
    /// Root a heap pointer using an explicit Context/handle context. No GC-header context lookup.
    #[inline]
    pub fn to_stack(self, mut cx: Context) -> StackRoot<T> {
        assert!(
            !self.is_dangling(),
            "to_stack() called on dangling/uninitialized HeapPtr!"
        );

        let handle_context = &mut cx.handle_context;
        StackRoot::new(handle_context, T::to_handle_contents(self))
    }
}

impl<T: IsHeapItem> From<StackRoot<T>> for StackRoot<Value> {
    #[inline]
    fn from(value: StackRoot<T>) -> Self {
        value.cast()
    }
}

/// Trait for items that can escape (be returned from) a handle scope. The item must be copied into
/// the parent handle scope.
pub trait Escapable {
    /// Copy this item into the current handle scope. Called from the parent's handle scope so that
    /// this item can escape the destroyed child handle scope.
    ///
    /// This is called after the handle scope containing the escaped item has been destroyed. This
    /// means that allocating a handle may overwrite the handles in this item. If multiple handles
    /// must be moved to the parent scope then be sure to copy out all the values before allocating
    /// any new handles, to avoid overwriting the old handles.
    fn escape(&self, cx: Context) -> Self;
}

impl Escapable for () {
    #[inline]
    fn escape(&self, _: Context) -> Self {}
}

impl Escapable for u32 {
    #[inline]
    fn escape(&self, _: Context) -> Self {
        *self
    }
}

impl Escapable for Value {
    #[inline]
    fn escape(&self, _: Context) -> Self {
        *self
    }
}

impl<T> Escapable for HeapPtr<T> {
    #[inline]
    fn escape(&self, _: Context) -> Self {
        *self
    }
}

impl Escapable for StackRoot<Value> {
    #[inline]
    fn escape(&self, cx: Context) -> Self {
        (**self).to_stack(cx)
    }
}

impl<T: IsHeapItem> Escapable for StackRoot<T> {
    #[inline]
    fn escape(&self, cx: Context) -> Self {
        // Re-root into the parent handle context explicitly using the provided Context.
        (**self).to_stack(cx)
    }
}

impl<T: Escapable> Escapable for Option<T> {
    #[inline]
    fn escape(&self, cx: Context) -> Self {
        self.as_ref().map(|some| some.escape(cx))
    }
}

impl<T: Escapable, E: Escapable> Escapable for Result<T, E> {
    #[inline]
    fn escape(&self, cx: Context) -> Self {
        match self {
            Ok(ok) => Ok(ok.escape(cx)),
            Err(err) => Err(err.escape(cx)),
        }
    }
}

impl<T: Escapable, U: Escapable> Escapable for (T, U) {
    #[inline]
    fn escape(&self, cx: Context) -> Self {
        (self.0.escape(cx), self.1.escape(cx))
    }
}
