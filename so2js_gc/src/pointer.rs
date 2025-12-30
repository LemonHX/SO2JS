//! GC-managed pointer type
//!
//! `GcPtr<T>` is a pointer to a GC-managed object. It should not be held on the stack
//! across potential GC points (allocations). Use `Handle<T>` for rooted references.

use core::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

/// A pointer to a GC-managed object.
///
/// This is a thin wrapper around a raw pointer. The object is managed by the GC
/// and may be freed if not reachable from roots.
///
/// # Safety
/// - Must not be held on the stack across GC points
/// - The pointed-to object must have a `GcHeader` immediately before it
#[repr(transparent)]
pub struct GcPtr<T> {
    ptr: NonNull<T>,
}

impl<T> GcPtr<T> {
    /// Get the raw pointer
    #[inline]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Create from a raw pointer
    ///
    /// # Safety
    /// The pointer must be non-null and point to a valid GC-managed object
    #[inline]
    pub const unsafe fn from_ptr(ptr: *mut T) -> GcPtr<T> {
        GcPtr {
            ptr: NonNull::new_unchecked(ptr),
        }
    }

    /// Create from a NonNull pointer
    #[inline]
    pub const fn from_non_null(ptr: NonNull<T>) -> GcPtr<T> {
        GcPtr { ptr }
    }

    /// Check pointer equality
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }

    /// Cast to another type
    #[inline]
    pub fn cast<U>(&self) -> GcPtr<U> {
        GcPtr {
            ptr: self.ptr.cast(),
        }
    }

    /// Create an uninitialized (dangling) pointer
    #[inline]
    pub const fn dangling() -> GcPtr<T> {
        GcPtr {
            ptr: NonNull::dangling(),
        }
    }

    /// Check if this is a dangling pointer
    #[inline]
    pub fn is_dangling(&self) -> bool {
        self.ptr == NonNull::dangling()
    }

    /// Get the underlying NonNull
    #[inline]
    pub fn as_non_null(&self) -> NonNull<T> {
        self.ptr
    }
}

impl<T> Clone for GcPtr<T> {
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GcPtr<T> {}

impl<T> Deref for GcPtr<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.ptr.as_ref() }
    }
}

impl<T> DerefMut for GcPtr<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.ptr.as_mut() }
    }
}

impl<T> core::fmt::Debug for GcPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "GcPtr({:p})", self.ptr)
    }
}

impl<T> core::fmt::Pointer for GcPtr<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Pointer::fmt(&self.ptr, f)
    }
}

/// Trait for types that can be traced by the GC
///
/// Implement this for types that contain GC pointers.
#[allow(dead_code)]
pub trait Trace {
    /// Visit all GC pointers in this object
    fn trace(&mut self, visitor: &mut dyn FnMut(GcPtr<u8>));
}
