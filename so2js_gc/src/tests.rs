//! GC Tests
//!
//! Tests for common GC scenarios that could cause memory leaks or corruption.

use alloc::vec::Vec;

use crate::visitor::{GcContext, GcVisitor};
use crate::{GcPhase, GcPtr, Heap};

/// A simple test object that can hold references to other objects
#[repr(C)]
struct TestObject {
    value: u64,
    next: Option<GcPtr<TestObject>>,
}

impl TestObject {
    fn new(value: u64) -> TestObject {
        TestObject { value, next: None }
    }

    fn visit_pointers(&self, visitor: &mut impl GcVisitor) {
        if let Some(mut next) = self.next {
            visitor.visit(&mut next);
        }
    }
}

/// A weak reference object
#[repr(C)]
struct WeakRefObject {
    weak_target: Option<GcPtr<TestObject>>,
}

/// Simulates a WeakMap entry
#[repr(C)]
struct WeakMapEntry {
    weak_key: Option<GcPtr<TestObject>>,
    value: Option<GcPtr<TestObject>>,
}

/// Simple test context implementing GcContext
/// Uses a list of root pointers
struct TestContext {
    roots: Vec<GcPtr<TestObject>>,
    weak_refs: Vec<GcPtr<WeakRefObject>>,
    weak_map_entries: Vec<GcPtr<WeakMapEntry>>,
}

impl TestContext {
    fn new() -> Self {
        TestContext {
            roots: Vec::new(),
            weak_refs: Vec::new(),
            weak_map_entries: Vec::new(),
        }
    }

    fn add_root(&mut self, ptr: GcPtr<TestObject>) {
        self.roots.push(ptr);
    }

    fn clear_roots(&mut self) {
        self.roots.clear();
    }
}

impl GcContext for TestContext {
    fn visit_roots(&mut self, visitor: &mut impl GcVisitor) {
        for root in &mut self.roots {
            visitor.visit(root);
        }
        for weak_ref in &mut self.weak_refs {
            // WeakRefObject itself is a root, but its target is weak
            let mut ptr: GcPtr<u8> = unsafe { GcPtr::from_ptr(weak_ref.as_ptr() as *mut u8) };
            visitor.visit(&mut ptr);
        }
        for entry in &mut self.weak_map_entries {
            let mut ptr: GcPtr<u8> = unsafe { GcPtr::from_ptr(entry.as_ptr() as *mut u8) };
            visitor.visit(&mut ptr);
        }
    }

    fn trace_object(&mut self, object_ptr: *mut u8, visitor: &mut impl GcVisitor) {
        // Check if this is a WeakRefObject - don't trace weak targets
        for weak_ref in &self.weak_refs {
            if weak_ref.as_ptr() as *mut u8 == object_ptr {
                // WeakRefObject has no strong references to trace
                return;
            }
        }

        // Check if this is a WeakMapEntry - don't trace weak keys or values
        for entry in &self.weak_map_entries {
            if entry.as_ptr() as *mut u8 == object_ptr {
                // WeakMapEntry: weak_key is weak, value is kept alive by key
                // So we don't trace anything here
                return;
            }
        }

        // Check if this is a TestObject in our roots
        for root in &self.roots {
            if root.as_ptr() as *mut u8 == object_ptr {
                unsafe {
                    let obj = &*root.as_ptr();
                    obj.visit_pointers(visitor);
                }
                return;
            }
        }

        // Try to trace it as a TestObject anyway (for linked objects not in roots)
        unsafe {
            let obj = &*(object_ptr as *const TestObject);
            obj.visit_pointers(visitor);
        }
    }

    fn process_weak_refs(&mut self, heap: &Heap) {
        for weak_ref in &self.weak_refs {
            unsafe {
                let wr = &mut *weak_ref.as_ptr();
                if let Some(target) = wr.weak_target {
                    if !heap.is_alive(target) {
                        wr.weak_target = None;
                    }
                }
            }
        }

        for entry in &self.weak_map_entries {
            unsafe {
                let e = &mut *entry.as_ptr();
                if let Some(k) = e.weak_key {
                    if !heap.is_alive(k) {
                        e.weak_key = None;
                        e.value = None;
                    }
                }
            }
        }
    }
}

// ============================================================================
// Basic allocation and collection tests
// ============================================================================

#[test]
fn test_basic_alloc() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let ptr = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        ptr.as_ptr().write(TestObject::new(42));
    }

    assert_eq!(heap.num_objects(), 1);
    assert!(heap.bytes_allocated() > 0);
}

#[test]
fn test_multiple_allocs() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    for i in 0..100 {
        let ptr = heap.alloc::<TestObject>(&mut ctx).unwrap();
        unsafe {
            ptr.as_ptr().write(TestObject::new(i));
        }
    }

    assert_eq!(heap.num_objects(), 100);
}

#[test]
fn test_collect_unreachable() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    for i in 0..10 {
        let ptr = heap.alloc::<TestObject>(&mut ctx).unwrap();
        unsafe {
            ptr.as_ptr().write(TestObject::new(i));
        }
    }

    assert_eq!(heap.num_objects(), 10);

    // GC with no roots - everything should be collected
    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 0);
    // Steps: 1 (marking: empty gray queue -> WeakRefProcessing)
    //      + 1 (weak refs -> Sweeping) + 1 (sweeping: 10 objects -> Idle)
    assert_eq!(steps, 3, "10 unreachable objects should take 3 GC steps");
    assert_eq!(heap.bytes_allocated(), 0);
}

#[test]
fn test_collect_rooted() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let root = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        root.as_ptr().write(TestObject::new(42));
    }

    for i in 0..10 {
        let ptr = heap.alloc::<TestObject>(&mut ctx).unwrap();
        unsafe {
            ptr.as_ptr().write(TestObject::new(i));
        }
    }

    assert_eq!(heap.num_objects(), 11);

    ctx.add_root(root);
    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 1);
    assert_eq!(root.value, 42);
    // Steps: 1 (marking: 1 root, then empty -> WeakRefProcessing)
    //      + 1 (weak refs -> Sweeping) + 1 (sweeping -> Idle)
    assert_eq!(steps, 3, "rooted GC should take 3 steps");
}

// ============================================================================
// Linked structure tests
// ============================================================================

#[test]
fn test_linked_list_reachable() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let head = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let a = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let b = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let c = heap.alloc::<TestObject>(&mut ctx).unwrap();

    unsafe {
        head.as_ptr().write(TestObject {
            value: 0,
            next: Some(a),
        });
        a.as_ptr().write(TestObject {
            value: 1,
            next: Some(b),
        });
        b.as_ptr().write(TestObject {
            value: 2,
            next: Some(c),
        });
        c.as_ptr().write(TestObject {
            value: 3,
            next: None,
        });
    }

    assert_eq!(heap.num_objects(), 4);

    ctx.add_root(head);
    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 4);
    assert_eq!(head.value, 0);
    assert_eq!(head.next.unwrap().value, 1);
    // Steps: 1 (marking: 4 objects, then empty -> WeakRefProcessing)
    //      + 1 (weak refs -> Sweeping) + 1 (sweeping -> Idle)
    assert_eq!(steps, 3, "linked list GC should take 3 steps");
}

#[test]
fn test_partial_list_unreachable() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let head = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let a = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let b = heap.alloc::<TestObject>(&mut ctx).unwrap();

    unsafe {
        head.as_ptr().write(TestObject {
            value: 0,
            next: Some(a),
        });
        a.as_ptr().write(TestObject {
            value: 1,
            next: Some(b),
        });
        b.as_ptr().write(TestObject {
            value: 2,
            next: None,
        });

        // Disconnect b
        (*a.as_ptr()).next = None;
    }

    assert_eq!(heap.num_objects(), 3);

    ctx.add_root(head);
    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    // b should be collected
    assert_eq!(heap.num_objects(), 2);
    // Steps: 1 (marking: 2 reachable, then empty) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "partial list GC should take 3 steps");
}

// ============================================================================
// Cycle tests
// ============================================================================

#[test]
fn test_simple_cycle_collected() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let a = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let b = heap.alloc::<TestObject>(&mut ctx).unwrap();

    unsafe {
        a.as_ptr().write(TestObject {
            value: 1,
            next: Some(b),
        });
        b.as_ptr().write(TestObject {
            value: 2,
            next: Some(a),
        });
    }

    assert_eq!(heap.num_objects(), 2);

    // GC with no roots - cycle should be collected
    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 0);
    // Steps: 1 (marking: empty -> WeakRefProcessing)
    //      + 1 (weak refs -> Sweeping) + 1 (sweeping -> Idle)
    assert_eq!(steps, 3, "cycle collection should take 3 steps");
}

#[test]
fn test_self_reference_collected() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let a = heap.alloc::<TestObject>(&mut ctx).unwrap();

    unsafe {
        a.as_ptr().write(TestObject {
            value: 1,
            next: Some(a),
        });
    }

    assert_eq!(heap.num_objects(), 1);

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 0);
    // Steps: 1 (marking: empty) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "self-reference collection should take 3 steps");
}

#[test]
fn test_rooted_cycle_survives() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let a = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let b = heap.alloc::<TestObject>(&mut ctx).unwrap();
    let c = heap.alloc::<TestObject>(&mut ctx).unwrap();

    unsafe {
        a.as_ptr().write(TestObject {
            value: 1,
            next: Some(b),
        });
        b.as_ptr().write(TestObject {
            value: 2,
            next: Some(c),
        });
        c.as_ptr().write(TestObject {
            value: 3,
            next: Some(a),
        });
    }

    assert_eq!(heap.num_objects(), 3);

    ctx.add_root(a);
    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 3);
    // Steps: 1 (marking: 3 objects) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "rooted cycle GC should take 3 steps");
}

#[test]
fn test_large_cycle_collected() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let n = 100;
    let mut objects: Vec<GcPtr<TestObject>> = Vec::new();

    for i in 0..n {
        let obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
        unsafe {
            obj.as_ptr().write(TestObject::new(i as u64));
        }
        objects.push(obj);
    }

    // Link them in a cycle
    for i in 0..n {
        unsafe {
            (*objects[i].as_ptr()).next = Some(objects[(i + 1) % n]);
        }
    }

    assert_eq!(heap.num_objects(), n);

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 0);
    // Steps: 1 (marking: empty) + 1 (weak refs)
    //      + 1 (sweeping: 100 objects, work_done=100, exit loop)
    //      + 1 (sweeping: empty -> Idle)
    assert_eq!(
        steps, 4,
        "large cycle collection should take 4 steps (100 objects hit sweep limit)"
    );
}

// ============================================================================
// Stress tests
// ============================================================================

#[test]
fn test_gc_stress_alloc_collect() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    for round in 0..10 {
        ctx.clear_roots();

        for i in 0..100 {
            let obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
            unsafe {
                obj.as_ptr().write(TestObject::new(i));
            }
            if i % 10 == 0 {
                ctx.add_root(obj);
            }
        }

        let expected_survivors = ctx.roots.len();

        heap.start_gc(&mut ctx);
        let steps = heap.finish_gc(&mut ctx);

        assert_eq!(
            heap.num_objects(),
            expected_survivors,
            "Round {}: expected {} survivors",
            round,
            expected_survivors
        );
        // Each round: 1 (marking) + 1 (weak refs)
        //           + 1 (sweeping: 100 objects) + 1 (sweeping: empty -> Idle)
        assert_eq!(
            steps, 4,
            "Round {}: stress GC should take 4 steps (100 objects)",
            round
        );
    }
}

#[test]
fn test_gc_stress_chain() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    fn build_chain(
        heap: &mut Heap,
        ctx: &mut TestContext,
        length: u32,
    ) -> Option<GcPtr<TestObject>> {
        if length == 0 {
            return None;
        }

        let node = heap.alloc::<TestObject>(ctx).unwrap();
        let tail = build_chain(heap, ctx, length - 1);

        unsafe {
            node.as_ptr().write(TestObject {
                value: length as u64,
                next: tail,
            });
        }

        Some(node)
    }

    let chain_length = 100;
    let root = build_chain(&mut heap, &mut ctx, chain_length);
    let initial_count = heap.num_objects();

    assert_eq!(initial_count, chain_length as usize);

    if let Some(r) = root {
        ctx.add_root(r);
    }
    heap.start_gc(&mut ctx);
    let steps1 = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), initial_count);
    // Steps: 1 (marking: 100 objects, hit limit)
    //      + 1 (marking: empty -> WeakRefProcessing)
    //      + 1 (weak refs -> Sweeping)
    //      + 1 (sweeping: 100 objects, hit limit)
    //      + 1 (sweeping: empty -> Idle)
    assert_eq!(
        steps1, 5,
        "chain trace should take 5 steps (100 objects hit both limits)"
    );

    // Now collect without root
    ctx.clear_roots();
    heap.start_gc(&mut ctx);
    let steps2 = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 0);
    // Steps: 1 (marking: empty) + 1 (weak refs)
    //      + 1 (sweeping: 100 objects) + 1 (sweeping: empty -> Idle)
    assert_eq!(
        steps2, 4,
        "chain sweep should take 4 steps (100 objects hit sweep limit)"
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_collect() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    assert_eq!(heap.num_objects(), 0);
    // Empty heap: marking finds nothing, sweeping finds nothing
    // Steps: 1 for marking (empty gray queue), 1 for weak refs, 1 for sweeping (empty list)
    assert!(
        steps <= 3,
        "empty GC should complete quickly, got {} steps",
        steps
    );
}

#[test]
fn test_collect_twice() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let _obj = heap.alloc::<TestObject>(&mut ctx).unwrap();

    heap.start_gc(&mut ctx);
    let steps1 = heap.finish_gc(&mut ctx);
    assert_eq!(heap.num_objects(), 0);
    assert!(steps1 >= 1, "first GC should do work");

    heap.start_gc(&mut ctx);
    let steps2 = heap.finish_gc(&mut ctx);
    assert_eq!(heap.num_objects(), 0);
    // Second GC on empty heap should be quick
    assert!(
        steps2 <= 3,
        "second GC on empty heap should be quick, got {} steps",
        steps2
    );
}

#[test]
fn test_alloc_after_collect() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    for _ in 0..10 {
        let _obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
    }

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);
    assert_eq!(heap.num_objects(), 0);
    // Steps: 1 (marking: empty) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "alloc after collect GC should take 3 steps");

    let obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        obj.as_ptr().write(TestObject::new(42));
    }
    assert_eq!(heap.num_objects(), 1);
    assert_eq!(obj.value, 42);
}

// ============================================================================
// Incremental GC tests
// ============================================================================

#[test]
fn test_incremental_gc_step_by_step() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    // Create objects
    for i in 0..50 {
        let obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
        unsafe {
            obj.as_ptr().write(TestObject::new(i));
        }
        if i < 10 {
            ctx.add_root(obj);
        }
    }

    assert_eq!(heap.num_objects(), 50);

    // Start GC
    heap.start_gc(&mut ctx);
    assert!(heap.gc_in_progress());
    assert_eq!(heap.phase(), GcPhase::Marking);

    // Step through GC
    let mut steps = 0;
    while heap.gc_step(&mut ctx) {
        steps += 1;
        if steps > 1000 {
            panic!("GC took too many steps");
        }
    }

    assert!(!heap.gc_in_progress());
    assert_eq!(heap.phase(), GcPhase::Idle);
    assert_eq!(heap.num_objects(), 10);
}

#[test]
fn test_alloc_during_gc_marks_black() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    // Create some objects
    for i in 0..10 {
        let obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
        unsafe {
            obj.as_ptr().write(TestObject::new(i));
        }
    }

    // Start GC but don't finish
    heap.start_gc(&mut ctx);
    assert!(heap.is_marking());

    // Allocate during marking - should be black (not collected this cycle)
    let new_obj = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        new_obj.as_ptr().write(TestObject::new(999));
    }

    // Finish GC
    let steps = heap.finish_gc(&mut ctx);

    // The new object should survive (it was black)
    assert_eq!(heap.num_objects(), 1);
    assert_eq!(new_obj.value, 999);
    // alloc() during GC advances 1 step, so finish_gc does remaining work
    // Total: mark(empty)->weak + weak->sweep + sweep(11 obj) + sweep(empty)->idle
    // But alloc() already did 1 step, so finish_gc returns fewer
    assert!(
        steps >= 1 && steps <= 4,
        "GC should complete in 1-4 steps, got {}",
        steps
    );
}

// ============================================================================
// Weak reference tests
// ============================================================================

#[test]
fn test_weak_ref_target_collected() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let target = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        target.as_ptr().write(TestObject::new(42));
    }

    let weak_ref = heap.alloc::<WeakRefObject>(&mut ctx).unwrap();
    unsafe {
        weak_ref.as_ptr().write(WeakRefObject {
            weak_target: Some(target),
        });
    }

    assert_eq!(heap.num_objects(), 2);

    // Root only the weak_ref, not the target
    ctx.weak_refs.push(weak_ref);

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    // Target should be collected, weak_ref should survive
    assert_eq!(heap.num_objects(), 1);
    assert!(weak_ref.weak_target.is_none());
    // Steps: 1 (marking: 1 weak_ref, then empty) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "weak ref collection should take 3 steps");
}

#[test]
fn test_weak_ref_target_survives_when_rooted() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let target = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        target.as_ptr().write(TestObject::new(42));
    }

    let weak_ref = heap.alloc::<WeakRefObject>(&mut ctx).unwrap();
    unsafe {
        weak_ref.as_ptr().write(WeakRefObject {
            weak_target: Some(target),
        });
    }

    // Root both
    ctx.add_root(target);
    ctx.weak_refs.push(weak_ref);

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    // Both should survive
    assert_eq!(heap.num_objects(), 2);
    assert!(weak_ref.weak_target.is_some());
    assert_eq!(weak_ref.weak_target.unwrap().value, 42);
    // Steps: 1 (marking: 2 objects) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "weak ref with rooted target should take 3 steps");
}

#[test]
fn test_weak_map_key_collected() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let key = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        key.as_ptr().write(TestObject::new(1));
    }

    let value = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        value.as_ptr().write(TestObject::new(2));
    }

    let entry = heap.alloc::<WeakMapEntry>(&mut ctx).unwrap();
    unsafe {
        entry.as_ptr().write(WeakMapEntry {
            weak_key: Some(key),
            value: Some(value),
        });
    }

    assert_eq!(heap.num_objects(), 3);

    // Root only the entry, not the key
    ctx.weak_map_entries.push(entry);

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    // Key and value should be collected, entry survives
    assert_eq!(heap.num_objects(), 1);
    assert!(entry.weak_key.is_none());
    assert!(entry.value.is_none());
    // Steps: 1 (marking: 1 entry, then empty) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "weak map key collection should take 3 steps");
}

#[test]
fn test_weak_map_key_survives_externally() {
    let mut heap = Heap::new();
    let mut ctx = TestContext::new();

    let key = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        key.as_ptr().write(TestObject::new(1));
    }

    let value = heap.alloc::<TestObject>(&mut ctx).unwrap();
    unsafe {
        value.as_ptr().write(TestObject::new(2));
    }

    let entry = heap.alloc::<WeakMapEntry>(&mut ctx).unwrap();
    unsafe {
        entry.as_ptr().write(WeakMapEntry {
            weak_key: Some(key),
            value: Some(value),
        });
    }

    // Root both entry and key
    ctx.add_root(key);
    ctx.weak_map_entries.push(entry);

    heap.start_gc(&mut ctx);
    let steps = heap.finish_gc(&mut ctx);

    // Entry survives, key survives, but value might be collected
    // (since we didn't implement ephemeron tracing in this simple test)
    assert!(entry.weak_key.is_some());
    // Steps: 1 (marking: 2 objects) + 1 (weak refs) + 1 (sweeping)
    assert_eq!(steps, 3, "weak map with rooted key should take 3 steps");
}
