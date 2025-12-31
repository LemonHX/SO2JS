use core::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use super::{StackRootContents, IsHeapItem, ToStackRootContents};

/// For direct references to heap pointers, such as references to other heap items stored within a
/// heap item. May not be held on stack during a GC (which can occur during any heap allocation).
///
/// This is a newtype wrapper around `so2js_gc::GcPtr<T>` that allows implementing
/// traits and methods specific to the so2js runtime.
#[repr(transparent)]
pub struct HeapPtr<T>(so2js_gc::GcPtr<T>);

impl<T> HeapPtr<T> {
    #[inline]
    pub const fn as_ptr(&self) -> *mut T {
        self.0.as_ptr()
    }

    #[inline]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub const fn from_ptr(ptr: *mut T) -> HeapPtr<T> {
        HeapPtr(so2js_gc::GcPtr::from_ptr(ptr))
    }

    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }

    #[inline]
    pub fn cast<U>(&self) -> HeapPtr<U> {
        HeapPtr(self.0.cast())
    }

    #[inline]
    pub fn cast_mut<U>(&mut self) -> &mut HeapPtr<U> {
        unsafe { core::mem::transmute(self) }
    }

    #[inline]
    pub const fn uninit() -> HeapPtr<T> {
        HeapPtr(so2js_gc::GcPtr::uninit())
    }

    /// Check if this is a dangling/uninitialized pointer
    #[inline]
    pub fn is_dangling(&self) -> bool {
        self.0.is_dangling()
    }

    /// Get the underlying NonNull pointer
    #[inline]
    pub fn as_non_null(&self) -> NonNull<T> {
        self.0.as_non_null()
    }

    /// Get the inner GcPtr (for interop with so2js_gc)
    #[inline]
    pub fn into_gc_ptr(self) -> so2js_gc::GcPtr<T> {
        self.0
    }

    /// Create from a GcPtr (for interop with so2js_gc)
    #[inline]
    pub const fn from_gc_ptr(ptr: so2js_gc::GcPtr<T>) -> HeapPtr<T> {
        HeapPtr(ptr)
    }
}

impl<T: IsHeapItem> ToStackRootContents for T {
    type Impl = HeapPtr<T>;

    #[inline]
    fn to_handle_contents(heap_ptr: HeapPtr<T>) -> StackRootContents {
        heap_ptr.as_ptr() as usize
    }
}

impl<T> Clone for HeapPtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for HeapPtr<T> {}

impl<T: IsHeapItem> Deref for HeapPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_non_null().as_ref() }
    }
}

impl<T: IsHeapItem> DerefMut for HeapPtr<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_non_null().as_mut() }
    }
}
