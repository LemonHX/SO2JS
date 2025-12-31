//! GC Visitor extension trait for so2js-specific types
//!
//! This provides convenience methods for visiting `Value`, `PropertyKey`, etc.
//! that are specific to the so2js runtime.

use core::mem::transmute;

use crate::runtime::{PropertyKey, Value};

// Re-export the base trait from so2js_gc
pub use so2js_gc::GcVisitor;

use super::HeapPtr;

/// Extension trait for GcVisitor with so2js-specific convenience methods
///
/// This trait is automatically implemented for all types that implement `GcVisitor`.
pub trait GcVisitorExt: GcVisitor {
    /// Visit a pointer to a Rust vtable.
    #[inline]
    fn visit_rust_vtable_pointer(&mut self, _ptr: &mut *const ()) {
        // Default: do nothing. vtables don't need to be traced.
    }

    /// Visit a strongly held HeapPtr
    #[inline]
    fn visit_pointer<T>(&mut self, ptr: &mut HeapPtr<T>) {
        if !ptr.is_dangling() {
            self.visit_raw(ptr.as_non_null().cast());
        }
    }

    /// Visit a weakly held HeapPtr
    #[inline]
    fn visit_weak_pointer<T>(&mut self, ptr: &mut HeapPtr<T>) {
        if !ptr.is_dangling() {
            self.visit_weak_raw(ptr.as_non_null().cast());
        }
    }

    /// Visit an optional strongly held HeapPtr
    #[inline]
    fn visit_pointer_opt<T>(&mut self, ptr: &mut Option<HeapPtr<T>>) {
        if let Some(p) = ptr {
            self.visit_pointer(p);
        }
    }

    /// Visit a strongly held value.
    #[inline]
    fn visit_value(&mut self, value: &mut Value) {
        if value.is_pointer() {
            unsafe {
                self.visit_raw(core::ptr::NonNull::new_unchecked(
                    transmute::<&mut Value, &mut *mut u8>(value).cast(),
                ));
            }
        }
    }

    /// Visit a weakly held value.
    #[inline]
    fn visit_weak_value(&mut self, value: &mut Value) {
        if value.is_pointer() {
            unsafe {
                self.visit_weak_raw(core::ptr::NonNull::new_unchecked(
                    transmute::<&mut Value, &mut *mut u8>(value).cast(),
                ));
            }
        }
    }

    /// Visit a strongly held property key.
    #[inline]
    fn visit_property_key(&mut self, property_key: &mut PropertyKey) {
        unsafe { self.visit_value(transmute::<&mut PropertyKey, &mut Value>(property_key)) };
    }
}

// Blanket implementation: any GcVisitor automatically gets GcVisitorExt
impl<T: GcVisitor> GcVisitorExt for T {}
