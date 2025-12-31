//! Heap - wraps so2js_gc::Heap with runtime-specific functionality
//!
//! This provides the memory allocation and GC interface for the so2js runtime.

use so2js_gc::GcContext;

use crate::runtime::{alloc_error::AllocResult, Context};

use super::{heap_item::AnyHeapItem, GcVisitorExt, HeapPtr};

#[cfg(feature = "alloc_error")]
use crate::runtime::alloc_error::AllocError;

/// Heap - wraps so2js_gc::Heap
pub struct Heap {
    /// The underlying GC heap
    gc_heap: so2js_gc::Heap,

    #[cfg(feature = "gc_stress_test")]
    pub gc_stress_test: bool,
}

impl Heap {
    pub fn new(_initial_size: usize) -> Heap {
        // Create the GC heap
        let gc_heap = so2js_gc::Heap::new();

        Heap {
            gc_heap,

            #[cfg(feature = "gc_stress_test")]
            gc_stress_test: false,
        }
    }

    pub fn alloc_uninit<T>(cx: Context) -> AllocResult<HeapPtr<T>> {
        Self::alloc_uninit_with_size::<T>(cx, size_of::<T>())
    }

    /// Allocate an object of a given type with the specified size in bytes.
    #[inline]
    pub fn alloc_uninit_with_size<T>(mut cx: Context, size: usize) -> AllocResult<HeapPtr<T>> {
        // Run a GC on every allocation in stress test mode
        #[cfg(feature = "gc_stress_test")]
        if cx.heap.gc_stress_test {
            Self::run_gc(cx);
        }

        // Get raw pointer to avoid borrow conflict
        let gc_heap_ptr = &mut cx.heap.gc_heap as *mut so2js_gc::Heap;

        // Try to allocate
        let result = unsafe { (*gc_heap_ptr).alloc_with_size::<T>(&mut RuntimeContext(cx), size) };

        match result {
            Ok(gc_ptr) => Ok(HeapPtr::from_gc_ptr(gc_ptr)),
            Err(_) => {
                // Run GC and try again
                Self::run_gc(cx);

                let result =
                    unsafe { (*gc_heap_ptr).alloc_with_size::<T>(&mut RuntimeContext(cx), size) };

                match result {
                    Ok(gc_ptr) => Ok(HeapPtr::from_gc_ptr(gc_ptr)),
                    Err(_) => {
                        #[cfg(feature = "alloc_error")]
                        {
                            Err(AllocError::oom())
                        }

                        #[cfg(not(feature = "alloc_error"))]
                        {
                            panic!("Ran out of heap memory");
                        }
                    }
                }
            }
        }
    }

    /// Run a full garbage collection cycle
    pub fn run_gc(mut cx: Context) {
        let mut ctx = RuntimeContext(cx);
        // Start GC and complete all steps
        cx.heap.gc_heap.start_gc(&mut ctx);
        cx.heap.gc_heap.finish_gc(&mut ctx);
    }

    /// Run incremental GC step
    pub fn gc_step(mut cx: Context) -> bool {
        let mut ctx = RuntimeContext(cx);
        cx.heap.gc_heap.gc_step(&mut ctx)
    }

    /// Get the underlying GC heap (for advanced operations)
    pub fn gc_heap(&self) -> &so2js_gc::Heap {
        &self.gc_heap
    }

    pub fn gc_heap_mut(&mut self) -> &mut so2js_gc::Heap {
        &mut self.gc_heap
    }
}

/// Wrapper to implement GcContext for Context
struct RuntimeContext(Context);

impl GcContext for RuntimeContext {
    fn visit_roots(&mut self, visitor: &mut impl so2js_gc::GcVisitor) {
        // Context::visit_roots_for_gc takes GcVisitorExt, which is a blanket impl on GcVisitor
        self.0.visit_roots_for_gc(visitor);
    }

    fn trace_object(&mut self, ptr: *mut u8, visitor: &mut impl so2js_gc::GcVisitor) {
        // Get the object as AnyHeapItem to read its descriptor
        let mut heap_item = HeapPtr::<AnyHeapItem>::from_ptr(ptr as *mut AnyHeapItem);
        let kind = heap_item.descriptor().kind();

        // Dispatch to the appropriate visit_pointers based on kind
        heap_item.visit_pointers_for_kind(visitor, kind);
    }

    fn process_weak_refs(&mut self, _heap: &so2js_gc::Heap) {
        // TODO: Implement weak reference processing
        // - Iterate through WeakRef objects, clear dead targets
        // - Clean up WeakMap/WeakSet entries with dead keys
        // - Trigger FinalizationRegistry callbacks
    }

    fn as_context_ptr(&mut self) -> *mut () {
        self.0.as_ptr() as *mut ()
    }
}
