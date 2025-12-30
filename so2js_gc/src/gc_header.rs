//! GC Header for tri-color marking GC
//!
//! Every heap object has a GcHeader prepended to track GC state.
//! Layout: | GcHeader | HeapItemDescriptor | ... object data ... |

use core::{alloc::Layout, ptr::NonNull};

/// The three colors used in tri-color marking
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum GcColor {
    /// White: Not yet visited, will be collected if still white after marking
    White = 0,
    /// Gray: Visited but children not yet scanned
    Gray = 1,
    /// Black: Visited and all children scanned
    Black = 2,
}

impl Default for GcColor {
    fn default() -> Self {
        GcColor::White
    }
}

/// GC phase for incremental collection
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GcPhase {
    /// No GC in progress
    Idle,
    /// Scanning root objects
    RootScanning,
    /// Incrementally marking gray objects
    Marking,
    /// Processing weak references (WeakRef, WeakMap, etc.)
    WeakRefProcessing,
    /// Incrementally sweeping white objects
    Sweeping,
}

impl Default for GcPhase {
    fn default() -> Self {
        GcPhase::Idle
    }
}

/// Header prepended to every heap object for GC tracking
///
/// This header is placed immediately before the object data in memory.
/// The HeapPtr points to the object data, so we need to offset back to find the header.
#[repr(C)]
pub struct GcHeader {
    /// Current color in tri-color marking
    color: GcColor,
    /// Padding for alignment
    _padding: [u8; 7],
    /// Size of allocation (object size, not including header)
    alloc_size: usize,
    /// Next object in the all-objects list (for sweeping)
    next_object: Option<NonNull<GcHeader>>,
}

impl GcHeader {
    /// Size of the GC header (must be aligned to 8 bytes)
    pub const SIZE: usize = core::mem::size_of::<GcHeader>();

    /// Alignment of allocations
    pub const ALIGN: usize = 8;

    /// Create a new GC header for an allocation
    #[inline]
    pub fn new(alloc_size: usize) -> GcHeader {
        GcHeader {
            color: GcColor::White,
            _padding: [0; 7],
            alloc_size,
            next_object: None,
        }
    }

    /// Get the color of this object
    #[inline]
    pub fn color(&self) -> GcColor {
        self.color
    }

    /// Set the color of this object
    #[inline]
    pub fn set_color(&mut self, color: GcColor) {
        self.color = color;
    }

    /// Get the object allocation size (not including header)
    #[inline]
    pub fn alloc_size(&self) -> usize {
        self.alloc_size
    }

    /// Get the total allocation size (including header)
    #[inline]
    pub fn total_size(&self) -> usize {
        Self::SIZE + self.alloc_size
    }

    /// Get the next object in the all-objects list
    #[inline]
    pub fn next_object(&self) -> Option<NonNull<GcHeader>> {
        self.next_object
    }

    /// Set the next object in the all-objects list
    #[inline]
    pub fn set_next_object(&mut self, next: Option<NonNull<GcHeader>>) {
        self.next_object = next;
    }

    /// Get a pointer to the object data (immediately after the header)
    #[inline]
    pub fn object_ptr(&self) -> *mut u8 {
        unsafe { (self as *const GcHeader as *mut u8).add(Self::SIZE) }
    }

    /// Get the GcHeader from an object pointer
    ///
    /// # Safety
    /// The object_ptr must point to a valid object allocated with a GcHeader
    #[inline]
    pub unsafe fn from_object_ptr<T>(object_ptr: *const T) -> &'static mut GcHeader {
        let header_ptr = (object_ptr as *mut u8).sub(Self::SIZE) as *mut GcHeader;
        &mut *header_ptr
    }

    /// Get the layout for an allocation of the given size
    #[inline]
    pub fn layout_for_size(size: usize) -> Layout {
        let total_size = Self::SIZE + align_up(size, Self::ALIGN);
        Layout::from_size_align(total_size, Self::ALIGN).unwrap()
    }

    /// Check if this object is marked (gray or black)
    #[inline]
    pub fn is_marked(&self) -> bool {
        self.color != GcColor::White
    }

    /// Check if this object needs scanning (is gray)
    #[inline]
    pub fn needs_scanning(&self) -> bool {
        self.color == GcColor::Gray
    }
}

/// Align a value up to the given alignment
#[inline]
fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gc_header_size() {
        // Ensure GcHeader is properly aligned (should be 24 bytes on 64-bit)
        assert_eq!(GcHeader::SIZE % 8, 0);
    }

    #[test]
    fn test_gc_header_color() {
        let mut header = GcHeader::new(64);
        assert_eq!(header.color(), GcColor::White);

        header.set_color(GcColor::Gray);
        assert_eq!(header.color(), GcColor::Gray);
        assert!(header.needs_scanning());

        header.set_color(GcColor::Black);
        assert_eq!(header.color(), GcColor::Black);
        assert!(header.is_marked());
        assert!(!header.needs_scanning());
    }
}
