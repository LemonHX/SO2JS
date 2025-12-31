use core::mem::size_of;

use crate::{
    extend_object,
    runtime::{
        alloc_error::AllocResult,
        collections::{BsHashMap, BsHashMapField},
        eval_result::EvalResult,
        gc::{GcVisitorExt, HeapItem},
        heap_item_descriptor::HeapItemKind,
        object_value::ObjectValue,
        ordinary_object::object_create_from_constructor,
        value::{ValueCollectionKey, ValueCollectionKeyStackRoot},
        Context, HeapPtr, StackRoot, Value,
    },
    set_uninit,
};

use super::intrinsics::Intrinsic;

// WeakMap Objects (https://tc39.es/ecma262/#sec-weakmap-objects)
extend_object! {
    pub struct WeakMapObject {
        // Map of weakly held keys to values. Can only hold object and symbols as keys.
        weak_map_data: HeapPtr<WeakValueMap>,
        // Holds the address of the next weak map that has been visited during garbage collection.
        // Unused outside of garbage collection.
        next_weak_map: Option<HeapPtr<WeakMapObject>>,
    }
}

type WeakValueMap = BsHashMap<ValueCollectionKey, Value>;

impl WeakMapObject {
    pub fn new_from_constructor(
        cx: Context,
        constructor: StackRoot<ObjectValue>,
    ) -> EvalResult<StackRoot<WeakMapObject>> {
        let weak_map_data =
            WeakValueMap::new_initial(cx, HeapItemKind::WeakMapObjectWeakValueMap)?.to_stack(cx);

        let mut object = object_create_from_constructor::<WeakMapObject>(
            cx,
            constructor,
            HeapItemKind::WeakMapObject,
            Intrinsic::WeakMapPrototype,
        )?;

        set_uninit!(object.weak_map_data, *weak_map_data);

        Ok(object.to_stack(cx))
    }

    pub fn weak_map_data(&self) -> HeapPtr<WeakValueMap> {
        self.weak_map_data
    }

    pub fn next_weak_map(&self) -> Option<HeapPtr<WeakMapObject>> {
        self.next_weak_map
    }

    pub fn set_next_weak_map(&mut self, next_weak_map: Option<HeapPtr<WeakMapObject>>) {
        self.next_weak_map = next_weak_map;
    }
}

impl StackRoot<WeakMapObject> {
    pub fn weak_map_data_field(&self) -> WeakMapObjectMapField {
        WeakMapObjectMapField(*self)
    }

    pub fn insert(
        &self,
        cx: Context,
        key: StackRoot<Value>,
        value: StackRoot<Value>,
    ) -> AllocResult<bool> {
        let key_handle = ValueCollectionKeyStackRoot::new(cx, key)?;

        Ok(self
            .weak_map_data_field()
            .maybe_grow_for_insertion(cx)?
            .insert_without_growing(key_handle.get(), *value))
    }
}

#[derive(Clone)]
pub struct WeakMapObjectMapField(StackRoot<WeakMapObject>);

impl BsHashMapField<ValueCollectionKey, Value> for WeakMapObjectMapField {
    fn new_map(&self, cx: Context, capacity: usize) -> AllocResult<HeapPtr<WeakValueMap>> {
        WeakValueMap::new(cx, HeapItemKind::WeakMapObjectWeakValueMap, capacity)
    }

    fn get(&self, _: Context) -> HeapPtr<WeakValueMap> {
        self.0.weak_map_data
    }

    fn set(&mut self, _: Context, map: HeapPtr<WeakValueMap>) {
        self.0.weak_map_data = map;
    }
}

impl HeapItem for HeapPtr<WeakMapObject> {
    fn byte_size(&self) -> usize {
        size_of::<WeakMapObject>()
    }

    fn visit_pointers(&mut self, visitor: &mut impl GcVisitorExt) {
        self.visit_object_pointers(visitor);
        visitor.visit_pointer(&mut self.weak_map_data);

        // Intentionally do not visit next_weak_map
    }
}

impl WeakMapObjectMapField {
    pub fn byte_size(map: &HeapPtr<WeakValueMap>) -> usize {
        WeakValueMap::calculate_size_in_bytes(map.capacity())
    }

    pub fn visit_pointers(map: &mut HeapPtr<WeakValueMap>, visitor: &mut impl GcVisitorExt) {
        map.visit_pointers(visitor);

        for (key, value) in map.iter_mut_gc_unsafe() {
            visitor.visit_weak_value(key.value_mut());
            visitor.visit_weak_value(value);
        }
    }
}
