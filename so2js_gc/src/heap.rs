//! Heap - GC-managed memory allocator with incremental collection
//!
//! Design:
//! - Uses alloc::alloc for memory allocation
//! - Maintains a linked list of all allocated objects
//! - Provides incremental tri-color mark-sweep garbage collection
//! - Allocation-driven GC: each alloc() call advances GC work
//!
//! The runtime provides:
//! - `GcContext::visit_roots` - enumerate root pointers
//! - `GcContext::trace_object` - trace pointers within an object

use core::ptr::NonNull;

use crate::{
    gc_header::{GcColor, GcHeader, GcPhase},
    gray_queue::GrayQueue,
    visitor::{GcContext, GcVisitor},
    GcPtr,
};

/// Default number of objects to process per GC step
const DEFAULT_MARK_STEP_SIZE: usize = 100;
const DEFAULT_SWEEP_STEP_SIZE: usize = 100;

/// Default GC threshold (1MB)
const DEFAULT_GC_THRESHOLD: usize = 1024 * 1024;

/// The managed heap with incremental GC
pub struct Heap {
    /// Head of the all-objects linked list
    all_objects: Option<NonNull<GcHeader>>,

    /// Number of bytes currently allocated
    pub bytes_allocated: usize,

    /// Number of objects currently allocated
    pub num_objects: usize,

    /// Threshold to trigger GC (in bytes)
    gc_threshold: usize,

    /// Gray queue for marking phase
    gray_queue: GrayQueue,

    /// Current GC phase
    pub phase: GcPhase,

    /// For incremental sweeping: current position in the all-objects list
    sweep_prev: Option<NonNull<GcHeader>>,
    sweep_current: Option<NonNull<GcHeader>>,

    /// Stats for current GC cycle
    pub bytes_freed_this_cycle: usize,
    pub objects_freed_this_cycle: usize,

    #[cfg(feature = "gc_stress_test")]
    pub gc_stress_test: bool,
}

/// Result type for allocations
pub type AllocResult<T> = Result<T, AllocError>;

/// Allocation error
#[derive(Debug)]
pub struct AllocError;

impl Heap {
    /// Create a new heap
    pub const fn new() -> Heap {
        Heap {
            all_objects: None,
            bytes_allocated: 0,
            num_objects: 0,
            gc_threshold: DEFAULT_GC_THRESHOLD,
            gray_queue: GrayQueue::new(),
            phase: GcPhase::Idle,
            sweep_prev: None,
            sweep_current: None,
            bytes_freed_this_cycle: 0,
            objects_freed_this_cycle: 0,

            #[cfg(feature = "gc_stress_test")]
            gc_stress_test: false,
        }
    }

    /// Get current GC phase
    #[inline]
    pub fn phase(&self) -> GcPhase {
        self.phase
    }

    /// Check if GC is in progress
    #[inline]
    pub fn gc_in_progress(&self) -> bool {
        self.phase != GcPhase::Idle
    }

    /// Check if we're in marking phase (for write barrier)
    #[inline]
    pub fn is_marking(&self) -> bool {
        matches!(self.phase, GcPhase::RootScanning | GcPhase::Marking)
    }

    /// Allocate memory for an object of type T
    ///
    /// Returns a pointer to uninitialized memory.
    /// The caller must initialize the object before any GC can occur.
    ///
    /// This also advances incremental GC if one is in progress.
    ///
    /// # Arguments
    /// * `ctx` - The runtime context for GC operations
    pub fn alloc<T>(&mut self, ctx: &mut impl GcContext) -> AllocResult<GcPtr<T>> {
        self.alloc_with_size(ctx, core::mem::size_of::<T>())
    }

    /// Allocate memory with the given size
    ///
    /// Layout: | GcHeader | object data ... |
    ///
    /// If GC is in marking phase, new objects are allocated BLACK
    /// to avoid being collected in the current cycle (floating garbage).
    ///
    /// # Arguments
    /// * `ctx` - The runtime context for GC operations
    /// * `size` - The size of the object in bytes
    pub fn alloc_with_size<T>(
        &mut self,
        ctx: &mut impl GcContext,
        size: usize,
    ) -> AllocResult<GcPtr<T>> {
        // Advance incremental GC if in progress
        if self.gc_in_progress() {
            self.gc_step(ctx);
        }

        let layout = GcHeader::layout_for_size(size);

        // Get context pointer for the GcHeader
        let context_ptr = ctx.as_context_ptr();

        unsafe {
            // Allocate memory
            let ptr = alloc::alloc::alloc(layout);
            if ptr.is_null() {
                return Err(AllocError);
            }

            // Initialize GcHeader
            let header = ptr as *mut GcHeader;
            header.write(GcHeader::new(size, context_ptr));
            let header_nn = NonNull::new_unchecked(header);

            // During GC, new objects are BLACK (won't be collected this cycle)
            // This applies to any GC phase, not just marking, because:
            // - In marking phase: prevents the object from being missed
            // - In sweeping phase: prevents the object from being immediately swept
            if self.gc_in_progress() {
                (*header).set_color(GcColor::Black);
            }

            // Link into all-objects list
            (*header).set_next_object(self.all_objects);
            self.all_objects = Some(header_nn);

            // Update stats
            self.bytes_allocated += (*header).total_size();
            self.num_objects += 1;

            // Return pointer to object data (after header)
            let object_ptr = ptr.add(GcHeader::SIZE) as *mut T;
            Ok(GcPtr::from_ptr(object_ptr))
        }
    }

    /// Check if GC should be triggered
    #[inline]
    pub fn should_gc(&self) -> bool {
        self.bytes_allocated > self.gc_threshold && self.phase == GcPhase::Idle
    }

    /// Get bytes currently allocated
    #[inline]
    pub fn bytes_allocated(&self) -> usize {
        self.bytes_allocated
    }

    /// Get number of objects currently allocated
    #[inline]
    pub fn num_objects(&self) -> usize {
        self.num_objects
    }

    // ========================================================================
    // Incremental GC API
    // ========================================================================

    /// Start an incremental GC cycle
    ///
    /// This initiates a new GC cycle by scanning roots.
    /// After calling this, use `gc_step()` to advance the GC incrementally,
    /// or call `finish_gc()` to complete synchronously.
    ///
    /// # Arguments
    /// * `ctx` - The runtime context that provides root scanning and object tracing
    ///
    /// # Example
    /// ```ignore
    /// if heap.should_gc() {
    ///     heap.start_gc(&mut context);
    /// }
    /// ```
    pub fn start_gc(&mut self, ctx: &mut impl GcContext) {
        if self.gc_in_progress() {
            return;
        }

        self.phase = GcPhase::RootScanning;
        self.bytes_freed_this_cycle = 0;
        self.objects_freed_this_cycle = 0;

        // Scan roots - this marks root objects gray
        {
            let mut marker = Marker {
                gray_queue: &mut self.gray_queue,
            };
            ctx.visit_roots(&mut marker);
        }

        // Transition to marking phase
        self.phase = GcPhase::Marking;
    }

    /// Advance incremental GC by one step
    ///
    /// Returns true if GC is still in progress, false if complete.
    ///
    /// # Arguments
    /// * `ctx` - The runtime context (required during Marking and WeakRefProcessing phases)
    ///
    /// # Example
    /// ```ignore
    /// // Allocation-driven GC: do some work on each allocation
    /// if heap.gc_in_progress() {
    ///     heap.gc_step(&mut context);
    /// }
    /// ```
    pub fn gc_step(&mut self, ctx: &mut impl GcContext) -> bool {
        match self.phase {
            GcPhase::Idle => false,
            GcPhase::RootScanning => {
                // Root scanning should complete in start_gc
                // This shouldn't happen, but handle it gracefully
                self.phase = GcPhase::Marking;
                true
            }
            GcPhase::Marking => {
                self.mark_step(ctx, DEFAULT_MARK_STEP_SIZE);
                self.phase != GcPhase::Idle
            }
            GcPhase::WeakRefProcessing => {
                // Process weak refs in one step (usually fast)
                ctx.process_weak_refs(self);
                // Start sweeping
                self.phase = GcPhase::Sweeping;
                self.sweep_prev = None;
                self.sweep_current = self.all_objects;
                true
            }
            GcPhase::Sweeping => {
                self.sweep_step(DEFAULT_SWEEP_STEP_SIZE);
                self.phase != GcPhase::Idle
            }
        }
    }

    /// Perform incremental marking
    ///
    /// Processes up to `work_limit` gray objects.
    fn mark_step(&mut self, ctx: &mut impl GcContext, work_limit: usize) {
        let mut work_done = 0;

        while work_done < work_limit {
            match self.gray_queue.pop() {
                Some(header_ptr) => {
                    unsafe {
                        let header = &mut *header_ptr.as_ptr();

                        // Mark black
                        header.set_color(GcColor::Black);

                        // Trace object's pointers
                        let object_ptr = header.object_ptr();
                        let mut marker = Marker {
                            gray_queue: &mut self.gray_queue,
                        };
                        ctx.trace_object(object_ptr, &mut marker);
                    }
                    work_done += 1;
                }
                None => {
                    // No more gray objects - marking complete
                    self.phase = GcPhase::WeakRefProcessing;
                    return;
                }
            }
        }
    }

    /// Perform incremental sweeping
    ///
    /// Processes up to `work_limit` objects.
    fn sweep_step(&mut self, work_limit: usize) {
        let mut work_done = 0;

        while work_done < work_limit {
            match self.sweep_current {
                Some(header_ptr) => {
                    unsafe {
                        let header = &mut *header_ptr.as_ptr();
                        let next = header.next_object();

                        if header.color() == GcColor::White {
                            // Dead object - unlink and free
                            match self.sweep_prev {
                                Some(p) => (*p.as_ptr()).set_next_object(next),
                                None => self.all_objects = next,
                            }

                            let layout = GcHeader::layout_for_size(header.alloc_size());
                            self.bytes_freed_this_cycle += header.total_size();
                            self.objects_freed_this_cycle += 1;

                            alloc::alloc::dealloc(header_ptr.as_ptr() as *mut u8, layout);
                            // Don't update sweep_prev
                        } else {
                            // Live object - reset to white for next cycle
                            header.set_color(GcColor::White);
                            self.sweep_prev = Some(header_ptr);
                        }

                        self.sweep_current = next;
                    }
                    work_done += 1;
                }
                None => {
                    // Sweeping complete
                    self.finish_sweep();
                    return;
                }
            }
        }
    }

    /// Finish sweeping and reset state
    fn finish_sweep(&mut self) {
        self.bytes_allocated -= self.bytes_freed_this_cycle;
        self.num_objects -= self.objects_freed_this_cycle;

        // Adjust threshold: GC when we've allocated 2x current live set
        self.gc_threshold = (self.bytes_allocated * 2).max(DEFAULT_GC_THRESHOLD);

        // Reset state
        self.phase = GcPhase::Idle;
        self.sweep_prev = None;
        self.sweep_current = None;
        self.bytes_freed_this_cycle = 0;
        self.objects_freed_this_cycle = 0;
    }

    /// Complete GC synchronously
    ///
    /// Runs all remaining GC work until complete.
    ///
    /// # Arguments
    /// * `ctx` - The runtime context
    ///
    /// # Returns
    /// The number of GC steps executed
    pub fn finish_gc(&mut self, ctx: &mut impl GcContext) -> usize {
        let mut steps = 0;
        loop {
            let in_progress = self.gc_step(ctx);
            steps += 1;
            if !in_progress {
                break;
            }
        }
        steps
    }

    // ========================================================================
    // Marking helpers
    // ========================================================================

    /// Mark an object gray (for root scanning and tracing)
    ///
    /// Call this for each pointer encountered during tracing.
    #[inline]
    pub fn mark_gray_ptr<T>(&mut self, ptr: GcPtr<T>) {
        if ptr.is_dangling() {
            return;
        }
        self.mark_gray_raw(ptr.as_ptr() as *mut u8);
    }

    /// Mark a raw pointer gray
    #[inline]
    pub fn mark_gray_raw(&mut self, object_ptr: *mut u8) {
        if object_ptr.is_null() {
            return;
        }

        unsafe {
            let header = GcHeader::from_object_ptr(object_ptr);
            if header.color() == GcColor::White {
                header.set_color(GcColor::Gray);
                self.gray_queue
                    .push(NonNull::new_unchecked(header as *mut GcHeader));
            }
        }
    }

    /// Write barrier - call when writing a pointer field
    ///
    /// During marking phase, this ensures the target is marked gray
    /// (Dijkstra-style insertion barrier).
    #[inline]
    pub fn write_barrier<T>(&mut self, target: GcPtr<T>) {
        if self.is_marking() && !target.is_dangling() {
            self.mark_gray_ptr(target);
        }
    }

    /// Write barrier for raw pointer
    #[inline]
    pub fn write_barrier_raw(&mut self, target: *mut u8) {
        if self.is_marking() && !target.is_null() {
            self.mark_gray_raw(target);
        }
    }

    // ========================================================================
    // Weak reference support
    // ========================================================================

    /// Check if an object is alive (black or gray) during GC
    ///
    /// Used by weak reference processing to check if targets are still alive.
    /// Call this only during the weak ref processing phase of GC.
    #[inline]
    pub fn is_alive<T>(&self, ptr: GcPtr<T>) -> bool {
        if ptr.is_dangling() {
            return false;
        }
        unsafe {
            let header = GcHeader::from_object_ptr(ptr.as_ptr() as *mut u8);
            header.color() != GcColor::White
        }
    }

    /// Check if an object is alive by raw pointer
    #[inline]
    pub fn is_alive_raw(&self, object_ptr: *mut u8) -> bool {
        if object_ptr.is_null() {
            return false;
        }
        unsafe {
            let header = GcHeader::from_object_ptr(object_ptr);
            header.color() != GcColor::White
        }
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Marker - implements GcVisitor for the marking phase
// ============================================================================

/// A marker that implements `GcVisitor` for use during GC.
///
/// This struct holds a mutable reference to the gray queue and is used
/// during root scanning and object tracing to mark reachable objects.
pub struct Marker<'a> {
    gray_queue: &'a mut GrayQueue,
}

impl<'a> Marker<'a> {
    /// Create a new marker from a Heap
    ///
    /// # Safety
    /// The caller must ensure the Heap is in a GC phase that allows marking.
    pub fn new(heap: &'a mut Heap) -> Self {
        Marker {
            gray_queue: &mut heap.gray_queue,
        }
    }
}

impl<'a> GcVisitor for Marker<'a> {
    fn visit_raw(&mut self, ptr: NonNull<u8>) {
        unsafe {
            let header = GcHeader::from_object_ptr(ptr.as_ptr());
            if (*header).color() == GcColor::White {
                (*header).set_color(GcColor::Gray);
                self.gray_queue
                    .push(NonNull::new_unchecked(header as *mut GcHeader));
            }
        }
    }

    fn visit_weak_raw(&mut self, _ptr: NonNull<u8>) {
        // Weak pointers are not traced during marking.
        // They will be processed later in the WeakRefProcessing phase.
    }
}
