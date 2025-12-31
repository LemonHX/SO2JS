mod heap_item;
mod heap_trait_object;
mod heap_visitor;
mod pointer;

// Re-export GcVisitor from so2js_gc, and our own GcVisitorExt extension
pub use heap_visitor::GcVisitorExt;
pub use so2js_gc::GcVisitor;

pub use crate::runtime::stack::{
    Escapable, StackRoot, StackRootContents, StackRootContext, StackRootScope, StackRootScopeGuard,
    ToStackRootContents,
};
pub use heap_item::{AnyHeapItem, HeapItem, IsHeapItem};
// HeapPtr is our own wrapper around so2js_gc::GcPtr
pub use pointer::HeapPtr;
