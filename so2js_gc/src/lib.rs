//! SO2JS Garbage Collector
//!
//! An incremental tri-color mark-sweep garbage collector.
//! This crate provides the core GC infrastructure without depending on the runtime types.
//!
//! Key types:
//! - `GcPtr<T>`: A pointer to a GC-managed object
//! - `GcHeader`: Header prepended to each allocation
//! - `Heap`: The managed heap
//!
//! Key traits:
//! - `GcVisitor`: Implemented by GC, used by objects to report pointers
//! - `GcContext`: Implemented by runtime, provides root scanning and object tracing

#![no_std]
extern crate alloc;

mod gc_header;
mod gray_queue;
mod heap;
mod pointer;
mod visitor;

pub use gc_header::{GcColor, GcHeader, GcPhase};
pub use heap::{AllocError, AllocResult, Heap, Marker};
pub use pointer::GcPtr;
pub use visitor::{GcContext, GcVisitor};

#[cfg(test)]
mod tests;
