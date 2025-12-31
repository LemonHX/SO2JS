//! GC Visitor and Context traits
//!
//! These traits allow the GC to be decoupled from the runtime types.
//! - `GcVisitor`: Implemented by the GC's Marker, used by objects to report their pointers
//! - `GcContext`: Implemented by the runtime (Context), provides root scanning and object tracing

use crate::GcPtr;

/// GC Visitor trait - implemented by the GC's marking logic
///
/// Objects call methods on this trait to report their pointers during tracing.
/// Similar to the original `HeapVisitor` trait.
///
/// # Example
/// ```ignore
/// impl HeapItem for MyObject {
///     fn visit_pointers(&mut self, visitor: &mut impl GcVisitor) {
///         visitor.visit(&mut self.field1);
///         visitor.visit_opt(&mut self.optional_field);
///         visitor.visit_weak(&mut self.weak_ref);
///     }
/// }
/// ```
pub trait GcVisitor {
    /// Visit a strongly held pointer
    ///
    /// Marks the target gray if it's white (not yet visited).
    fn visit<T>(&mut self, ptr: &mut GcPtr<T>);

    /// Visit a weakly held pointer
    ///
    /// Does NOT trace the target. During marking, weak pointers are recorded
    /// for later processing. After marking completes, weak references to
    /// unreachable objects will be cleared.
    fn visit_weak<T>(&mut self, ptr: &mut GcPtr<T>) {
        // Default: do nothing for weak pointers during marking
        let _ = ptr;
    }

    /// Visit an optional strongly held pointer
    #[inline]
    fn visit_opt<T>(&mut self, ptr: &mut Option<GcPtr<T>>) {
        if let Some(p) = ptr {
            self.visit(p);
        }
    }

    /// Visit an optional weak pointer
    #[inline]
    fn visit_weak_opt<T>(&mut self, ptr: &mut Option<GcPtr<T>>) {
        if let Some(p) = ptr {
            self.visit_weak(p);
        }
    }
}

/// GC Context trait - implemented by the runtime (e.g., Context)
///
/// Provides the GC with access to roots and object tracing logic.
/// This separates the GC algorithm from the runtime's type system.
///
/// # Example
/// ```ignore
/// impl GcContext for Context {
///     fn visit_roots(&mut self, visitor: &mut impl GcVisitor) {
///         visitor.visit(&mut self.global);
///         for handle in &mut self.handle_scope {
///             visitor.visit(handle);
///         }
///     }
///     
///     fn trace_object(&mut self, object_ptr: *mut u8, visitor: &mut impl GcVisitor) {
///         let header = unsafe { &*(object_ptr as *const HeapItemHeader) };
///         match header.class() {
///             HeapItemClass::String => { /* strings don't have pointers */ },
///             HeapItemClass::Object => {
///                 let obj = unsafe { &mut *(object_ptr as *mut ObjectValue) };
///                 obj.visit_pointers(visitor);
///             },
///             // ... other types
///         }
///     }
/// }
/// ```
pub trait GcContext {
    /// Visit all root objects
    ///
    /// Called at the start of GC to mark all root objects gray.
    /// Implementation should call `visitor.visit()` for each root pointer.
    ///
    /// Roots typically include:
    /// - Global object
    /// - Active handle scopes  
    /// - Stack frames and registers
    /// - Compiler/parser temporary values
    fn visit_roots(&mut self, visitor: &mut impl GcVisitor);

    /// Trace an object's pointers
    ///
    /// Called for each gray object during marking.
    /// `object_ptr` points to the object data (after GcHeader).
    ///
    /// Implementation should:
    /// 1. Determine the object's type (from HeapItemHeader or type tag)
    /// 2. Cast to the concrete type
    /// 3. Call `visit_pointers` on the object
    fn trace_object(&mut self, object_ptr: *mut u8, visitor: &mut impl GcVisitor);

    /// Process weak references after marking is complete
    ///
    /// Called after all reachable objects are marked, before sweeping.
    ///
    /// Implementation should:
    /// - Clear WeakRefs whose targets are white (dead)
    /// - Remove WeakMap entries with unreachable keys
    /// - Remove WeakSet entries that are unreachable
    /// - Queue FinalizationRegistry callbacks for dead objects
    ///
    /// Use `Heap::is_alive()` to check if an object survived the marking phase.
    fn process_weak_refs(&mut self, heap: &crate::Heap) {
        // Default: do nothing
        let _ = heap;
    }
}
