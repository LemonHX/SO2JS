//! Gray Queue for tri-color marking
//!
//! Simple Vec-based queue of gray objects waiting to be scanned.
//! Using Vec is simpler and has better cache locality than an intrusive list.

use alloc::vec::Vec;
use core::ptr::NonNull;

use crate::gc_header::GcHeader;

/// Queue of gray objects to be scanned
pub struct GrayQueue {
    queue: Vec<NonNull<GcHeader>>,
}

impl GrayQueue {
    /// Create a new empty gray queue
    pub const fn new() -> GrayQueue {
        GrayQueue { queue: Vec::new() }
    }

    /// Check if the queue is empty
    #[inline]
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get the number of items in the queue
    #[inline]
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Push an object onto the queue
    #[inline]
    pub fn push(&mut self, header: NonNull<GcHeader>) {
        self.queue.push(header);
    }

    /// Pop an object from the queue
    #[inline]
    pub fn pop(&mut self) -> Option<NonNull<GcHeader>> {
        self.queue.pop()
    }

    /// Clear the queue
    #[inline]
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl Default for GrayQueue {
    fn default() -> Self {
        Self::new()
    }
}
