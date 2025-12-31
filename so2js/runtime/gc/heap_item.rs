use crate::runtime::{
    accessor::Accessor,
    arguments_object::{MappedArgumentsObject, UnmappedArgumentsObject},
    array_object::ArrayObject,
    array_properties::{DenseArrayProperties, SparseArrayProperties},
    async_generator_object::{AsyncGeneratorObject, AsyncGeneratorRequest},
    boxed_value::BoxedValue,
    bytecode::{
        constant_table::ConstantTable,
        exception_handlers::ExceptionStackRootrs,
        function::{BytecodeFunction, Closure},
    },
    class_names::ClassNames,
    collections::{
        array::{byte_array_visit_pointers, u32_array_visit_pointers, value_array_visit_pointers},
        vec::value_vec_visit_pointers,
    },
    context::{GlobalSymbolRegistryField, ModuleCacheField},
    for_in_iterator::ForInIterator,
    generator_object::GeneratorObject,
    global_names::GlobalNames,
    heap_item_descriptor::{HeapItemDescriptor, HeapItemKind},
    interned_strings::InternedStringsSetField,
    intrinsics::{
        array_buffer_constructor::ArrayBufferObject,
        array_iterator::ArrayIterator,
        async_from_sync_iterator_prototype::AsyncFromSyncIterator,
        bigint_constructor::BigIntObject,
        boolean_constructor::BooleanObject,
        data_view_constructor::DataViewObject,
        date_object::DateObject,
        error_constructor::ErrorObject,
        finalization_registry_object::{FinalizationRegistryCells, FinalizationRegistryObject},
        iterator_constructor::WrappedValidIterator,
        iterator_helper_object::IteratorHelperObject,
        map_iterator::MapIterator,
        map_object::{MapObject, MapObjectMapField},
        number_constructor::NumberObject,
        object_prototype::ObjectPrototype,
        regexp_constructor::RegExpObject,
        regexp_string_iterator::RegExpStringIterator,
        set_iterator::SetIterator,
        set_object::{SetObject, SetObjectSetField},
        string_iterator::StringIterator,
        symbol_constructor::SymbolObject,
        typed_array::{
            BigInt64Array, BigUInt64Array, Float16Array, Float32Array, Float64Array, Int16Array,
            Int32Array, Int8Array, UInt16Array, UInt32Array, UInt8Array, UInt8ClampedArray,
        },
        weak_map_object::{WeakMapObject, WeakMapObjectMapField},
        weak_ref_constructor::WeakRefObject,
        weak_set_object::{WeakSetObject, WeakSetObjectSetField},
    },
    module::{
        import_attributes::ImportAttributes,
        module_namespace_object::ModuleNamespaceObject,
        source_text_module::{
            module_option_array_visit_pointers, module_request_array_visit_pointers,
            ExportMapField, SourceTextModule,
        },
        synthetic_module::SyntheticModule,
    },
    object_value::{NamedPropertiesMapField, ObjectValue},
    promise_object::{PromiseCapability, PromiseObject, PromiseReaction},
    proxy_object::ProxyObject,
    realm::{GlobalScopes, LexicalNamesMapField},
    regexp::compiled_regexp::CompiledRegExpObject,
    scope::Scope,
    scope_names::ScopeNames,
    source_file::SourceFile,
    stack_trace::stack_frame_info_array_visit_pointers,
    string_object::StringObject,
    string_value::StringValue,
    value::{BigIntValue, SymbolValue},
    Realm,
};

use super::{GcVisitorExt, HeapPtr};

/// Trait implemented by all items stored on the heap. This includes both JS objects and non-object
/// items like strings and descriptors.
pub trait HeapItem {
    /// Size of this heap item in bytes. Not guaranteed to be aligned.
    fn byte_size(&self) -> usize;

    /// Call the provided visit function on all pointer fields in this item. Pass a mutable
    /// reference to the fields themselves so they can be updated in copying collection.
    fn visit_pointers(&mut self, visitor: &mut impl GcVisitorExt);
}

/// An arbitrary heap item. Only common field between heap items is their descriptor, which can be
/// used to determine the true type of the heap item.
#[repr(C)]
pub struct AnyHeapItem {
    descriptor: HeapPtr<HeapItemDescriptor>,
}

impl AnyHeapItem {
    pub fn descriptor(&self) -> HeapPtr<HeapItemDescriptor> {
        self.descriptor
    }

    pub fn set_descriptor(&mut self, descriptor: HeapPtr<HeapItemDescriptor>) {
        self.descriptor = descriptor;
    }
}

impl HeapPtr<AnyHeapItem> {
    pub fn visit_pointers_for_kind(&mut self, visitor: &mut impl GcVisitorExt, kind: HeapItemKind) {
        match kind {
            HeapItemKind::Descriptor => self.cast::<HeapItemDescriptor>().visit_pointers(visitor),
            HeapItemKind::OrdinaryObject => self.cast::<ObjectValue>().visit_pointers(visitor),
            HeapItemKind::Proxy => self.cast::<ProxyObject>().visit_pointers(visitor),
            HeapItemKind::BooleanObject => self.cast::<BooleanObject>().visit_pointers(visitor),
            HeapItemKind::NumberObject => self.cast::<NumberObject>().visit_pointers(visitor),
            HeapItemKind::StringObject => self.cast::<StringObject>().visit_pointers(visitor),
            HeapItemKind::SymbolObject => self.cast::<SymbolObject>().visit_pointers(visitor),
            HeapItemKind::BigIntObject => self.cast::<BigIntObject>().visit_pointers(visitor),
            HeapItemKind::ArrayObject => self.cast::<ArrayObject>().visit_pointers(visitor),
            HeapItemKind::RegExpObject => self.cast::<RegExpObject>().visit_pointers(visitor),
            HeapItemKind::ErrorObject => self.cast::<ErrorObject>().visit_pointers(visitor),
            HeapItemKind::DateObject => self.cast::<DateObject>().visit_pointers(visitor),
            HeapItemKind::SetObject => self.cast::<SetObject>().visit_pointers(visitor),
            HeapItemKind::MapObject => self.cast::<MapObject>().visit_pointers(visitor),
            HeapItemKind::WeakRefObject => self.cast::<WeakRefObject>().visit_pointers(visitor),
            HeapItemKind::WeakSetObject => self.cast::<WeakSetObject>().visit_pointers(visitor),
            HeapItemKind::WeakMapObject => self.cast::<WeakMapObject>().visit_pointers(visitor),
            HeapItemKind::FinalizationRegistryObject => self
                .cast::<FinalizationRegistryObject>()
                .visit_pointers(visitor),
            HeapItemKind::MappedArgumentsObject => {
                self.cast::<MappedArgumentsObject>().visit_pointers(visitor)
            }
            HeapItemKind::UnmappedArgumentsObject => self
                .cast::<UnmappedArgumentsObject>()
                .visit_pointers(visitor),
            HeapItemKind::Int8Array => self.cast::<Int8Array>().visit_pointers(visitor),
            HeapItemKind::UInt8Array => self.cast::<UInt8Array>().visit_pointers(visitor),
            HeapItemKind::UInt8ClampedArray => {
                self.cast::<UInt8ClampedArray>().visit_pointers(visitor)
            }
            HeapItemKind::Int16Array => self.cast::<Int16Array>().visit_pointers(visitor),
            HeapItemKind::UInt16Array => self.cast::<UInt16Array>().visit_pointers(visitor),
            HeapItemKind::Int32Array => self.cast::<Int32Array>().visit_pointers(visitor),
            HeapItemKind::UInt32Array => self.cast::<UInt32Array>().visit_pointers(visitor),
            HeapItemKind::BigInt64Array => self.cast::<BigInt64Array>().visit_pointers(visitor),
            HeapItemKind::BigUInt64Array => self.cast::<BigUInt64Array>().visit_pointers(visitor),
            HeapItemKind::Float16Array => self.cast::<Float16Array>().visit_pointers(visitor),
            HeapItemKind::Float32Array => self.cast::<Float32Array>().visit_pointers(visitor),
            HeapItemKind::Float64Array => self.cast::<Float64Array>().visit_pointers(visitor),
            HeapItemKind::ArrayBufferObject => {
                self.cast::<ArrayBufferObject>().visit_pointers(visitor)
            }
            HeapItemKind::DataViewObject => self.cast::<DataViewObject>().visit_pointers(visitor),
            HeapItemKind::ArrayIterator => self.cast::<ArrayIterator>().visit_pointers(visitor),
            HeapItemKind::StringIterator => self.cast::<StringIterator>().visit_pointers(visitor),
            HeapItemKind::SetIterator => self.cast::<SetIterator>().visit_pointers(visitor),
            HeapItemKind::MapIterator => self.cast::<MapIterator>().visit_pointers(visitor),
            HeapItemKind::RegExpStringIterator => {
                self.cast::<RegExpStringIterator>().visit_pointers(visitor)
            }
            HeapItemKind::ForInIterator => self.cast::<ForInIterator>().visit_pointers(visitor),
            HeapItemKind::AsyncFromSyncIterator => {
                self.cast::<AsyncFromSyncIterator>().visit_pointers(visitor)
            }
            HeapItemKind::WrappedValidIterator => {
                self.cast::<WrappedValidIterator>().visit_pointers(visitor)
            }
            HeapItemKind::IteratorHelperObject => {
                self.cast::<IteratorHelperObject>().visit_pointers(visitor)
            }
            HeapItemKind::ObjectPrototype => self.cast::<ObjectPrototype>().visit_pointers(visitor),
            HeapItemKind::String => self.cast::<StringValue>().visit_pointers(visitor),
            HeapItemKind::Symbol => self.cast::<SymbolValue>().visit_pointers(visitor),
            HeapItemKind::BigInt => self.cast::<BigIntValue>().visit_pointers(visitor),
            HeapItemKind::Accessor => self.cast::<Accessor>().visit_pointers(visitor),
            HeapItemKind::Promise => self.cast::<PromiseObject>().visit_pointers(visitor),
            HeapItemKind::PromiseReaction => self.cast::<PromiseReaction>().visit_pointers(visitor),
            HeapItemKind::PromiseCapability => {
                self.cast::<PromiseCapability>().visit_pointers(visitor)
            }
            HeapItemKind::Realm => self.cast::<Realm>().visit_pointers(visitor),
            HeapItemKind::Closure => self.cast::<Closure>().visit_pointers(visitor),
            HeapItemKind::BytecodeFunction => {
                self.cast::<BytecodeFunction>().visit_pointers(visitor)
            }
            HeapItemKind::ConstantTable => self.cast::<ConstantTable>().visit_pointers(visitor),
            HeapItemKind::ExceptionStackRootrs => {
                self.cast::<ExceptionStackRootrs>().visit_pointers(visitor)
            }
            HeapItemKind::SourceFile => self.cast::<SourceFile>().visit_pointers(visitor),
            HeapItemKind::Scope => self.cast::<Scope>().visit_pointers(visitor),
            HeapItemKind::ScopeNames => self.cast::<ScopeNames>().visit_pointers(visitor),
            HeapItemKind::GlobalNames => self.cast::<GlobalNames>().visit_pointers(visitor),
            HeapItemKind::ClassNames => self.cast::<ClassNames>().visit_pointers(visitor),
            HeapItemKind::SourceTextModule => {
                self.cast::<SourceTextModule>().visit_pointers(visitor)
            }
            HeapItemKind::SyntheticModule => self.cast::<SyntheticModule>().visit_pointers(visitor),
            HeapItemKind::ModuleNamespaceObject => {
                self.cast::<ModuleNamespaceObject>().visit_pointers(visitor)
            }
            HeapItemKind::ImportAttributes => {
                self.cast::<ImportAttributes>().visit_pointers(visitor)
            }
            HeapItemKind::Generator => self.cast::<GeneratorObject>().visit_pointers(visitor),
            HeapItemKind::AsyncGenerator => {
                self.cast::<AsyncGeneratorObject>().visit_pointers(visitor)
            }
            HeapItemKind::AsyncGeneratorRequest => {
                self.cast::<AsyncGeneratorRequest>().visit_pointers(visitor)
            }
            HeapItemKind::DenseArrayProperties => {
                self.cast::<DenseArrayProperties>().visit_pointers(visitor)
            }
            HeapItemKind::SparseArrayProperties => {
                self.cast::<SparseArrayProperties>().visit_pointers(visitor)
            }
            HeapItemKind::CompiledRegExpObject => {
                self.cast::<CompiledRegExpObject>().visit_pointers(visitor)
            }
            HeapItemKind::BoxedValue => self.cast::<BoxedValue>().visit_pointers(visitor),
            HeapItemKind::ObjectNamedPropertiesMap => {
                NamedPropertiesMapField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::MapObjectValueMap => {
                MapObjectMapField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::SetObjectValueSet => {
                SetObjectSetField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::ExportMap => ExportMapField::visit_pointers(self.cast_mut(), visitor),
            HeapItemKind::WeakMapObjectWeakValueMap => {
                WeakMapObjectMapField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::WeakSetObjectWeakValueSet => {
                WeakSetObjectSetField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::GlobalSymbolRegistryMap => {
                GlobalSymbolRegistryField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::InternedStringsSet => {
                InternedStringsSetField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::LexicalNamesMap => {
                LexicalNamesMapField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::ModuleCacheMap => {
                ModuleCacheField::visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::ValueArray => value_array_visit_pointers(self.cast_mut(), visitor),
            HeapItemKind::ByteArray => byte_array_visit_pointers(self.cast_mut(), visitor),
            HeapItemKind::U32Array => u32_array_visit_pointers(self.cast_mut(), visitor),
            HeapItemKind::ModuleRequestArray => {
                module_request_array_visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::ModuleOptionArray => {
                module_option_array_visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::StackFrameInfoArray => {
                stack_frame_info_array_visit_pointers(self.cast_mut(), visitor)
            }
            HeapItemKind::FinalizationRegistryCells => self
                .cast::<FinalizationRegistryCells>()
                .visit_pointers(visitor),
            HeapItemKind::GlobalScopes => self.cast::<GlobalScopes>().visit_pointers(visitor),
            HeapItemKind::ValueVec => value_vec_visit_pointers(self.cast_mut(), visitor),
            HeapItemKind::Last => unreachable!("No objects are created with this descriptor"),
        }
    }
}

impl HeapItem for HeapPtr<AnyHeapItem> {
    fn byte_size(&self) -> usize {
        self.descriptor().byte_size_for_item(*self)
    }

    fn visit_pointers(&mut self, visitor: &mut impl GcVisitorExt) {
        self.visit_pointers_for_kind(visitor, self.descriptor().kind());
    }
}

/// Marker trait that denotes an object on the managed heap
pub trait IsHeapItem {}

impl<T> IsHeapItem for T where HeapPtr<T>: HeapItem {}

impl<T> HeapPtr<T>
where
    HeapPtr<T>: HeapItem,
{
    #[inline]
    pub fn as_heap_item(&self) -> HeapPtr<AnyHeapItem> {
        self.cast()
    }
}
