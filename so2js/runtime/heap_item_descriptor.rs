use alloc::vec;
use alloc::vec::Vec;
use core::mem::size_of;

use bitflags::bitflags;

use crate::{
    runtime::{
        accessor::Accessor,
        alloc_error::AllocResult,
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
            array::{
                byte_array_byte_size, module_option_array_byte_size,
                module_request_array_byte_size, u32_array_byte_size, value_array_byte_size,
            },
            vec::value_vec_byte_size,
        },
        context::{GlobalSymbolRegistryField, ModuleCacheField},
        for_in_iterator::ForInIterator,
        generator_object::GeneratorObject,
        global_names::GlobalNames,
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
                BigInt64Array, BigUInt64Array, Float16Array, Float32Array, Float64Array,
                Int16Array, Int32Array, Int8Array, UInt16Array, UInt32Array, UInt8Array,
                UInt8ClampedArray,
            },
            weak_map_object::{WeakMapObject, WeakMapObjectMapField},
            weak_ref_constructor::WeakRefObject,
            weak_set_object::{WeakSetObject, WeakSetObjectSetField},
        },
        module::{
            import_attributes::ImportAttributes,
            module_namespace_object::ModuleNamespaceObject,
            source_text_module::{ExportMapField, SourceTextModule},
            synthetic_module::SyntheticModule,
        },
        object_value::{NamedPropertiesMapField, ObjectValue, VirtualObject, VirtualObjectVtable},
        promise_object::{PromiseCapability, PromiseObject, PromiseReaction},
        proxy_object::ProxyObject,
        realm::{GlobalScopes, LexicalNamesMapField},
        regexp::compiled_regexp::CompiledRegExpObject,
        rust_vtables::extract_virtual_object_vtable,
        scope::Scope,
        scope_names::ScopeNames,
        source_file::SourceFile,
        stack_trace::stack_frame_info_array_byte_size,
        string_object::StringObject,
        string_value::StringValue,
        value::{BigIntValue, SymbolValue},
        Context, Value,
    },
    set_uninit,
};

use super::{
    array_object::ArrayObject,
    gc::{AnyHeapItem, GcVisitorExt, HeapItem, HeapPtr, StackRoot},
    intrinsics::typed_array::{
        BigInt64Array, BigUInt64Array, Float16Array, Float32Array, Float64Array, Int16Array,
        Int32Array, Int8Array, UInt16Array, UInt32Array, UInt8Array, UInt8ClampedArray,
    },
    object_value::{VirtualObject, VirtualObjectVtable},
    proxy_object::ProxyObject,
    string_object::StringObject,
    Context,
};

#[repr(C)]
pub struct HeapItemDescriptor {
    /// Always the singleton descriptor descriptor
    descriptor: HeapPtr<HeapItemDescriptor>,
    /// Rust VirtualObject vtable, used for dynamic dispatch to some object methods
    vtable: VirtualObjectVtable,
    /// Object's type
    kind: HeapItemKind,
    /// Bitflags for object
    flags: DescFlags,
}

/// Type of an item in the heap. May be a JS object or non-object data stored on the heap,
/// e.g. descriptors and realms.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum HeapItemKind {
    // The descriptor for a descriptor
    Descriptor,

    // All objects
    OrdinaryObject,
    Proxy,

    BooleanObject,
    NumberObject,
    StringObject,
    SymbolObject,
    BigIntObject,
    ArrayObject,
    RegExpObject,
    ErrorObject,
    DateObject,
    SetObject,
    MapObject,
    WeakRefObject,
    WeakSetObject,
    WeakMapObject,
    FinalizationRegistryObject,

    MappedArgumentsObject,
    UnmappedArgumentsObject,

    Int8Array,
    UInt8Array,
    UInt8ClampedArray,
    Int16Array,
    UInt16Array,
    Int32Array,
    UInt32Array,
    BigInt64Array,
    BigUInt64Array,
    Float16Array,
    Float32Array,
    Float64Array,

    ArrayBufferObject,
    DataViewObject,

    ArrayIterator,
    StringIterator,
    SetIterator,
    MapIterator,
    RegExpStringIterator,
    ForInIterator,
    AsyncFromSyncIterator,
    WrappedValidIterator,
    IteratorHelperObject,

    ObjectPrototype,

    // Other heap items
    String,
    Symbol,
    BigInt,
    Accessor,

    Promise,
    PromiseReaction,
    PromiseCapability,

    Realm,

    Closure,
    BytecodeFunction,
    ConstantTable,
    ExceptionStackRootrs,
    SourceFile,

    Scope,
    ScopeNames,
    GlobalNames,
    ClassNames,

    SourceTextModule,
    SyntheticModule,
    ModuleNamespaceObject,
    ImportAttributes,

    Generator,
    AsyncGenerator,
    AsyncGeneratorRequest,

    DenseArrayProperties,
    SparseArrayProperties,

    CompiledRegExpObject,

    BoxedValue,

    // Hash maps
    ObjectNamedPropertiesMap,
    MapObjectValueMap,
    SetObjectValueSet,
    ExportMap,
    WeakSetObjectWeakValueSet,
    WeakMapObjectWeakValueMap,
    GlobalSymbolRegistryMap,
    InternedStringsSet,
    LexicalNamesMap,
    ModuleCacheMap,

    // Arrays
    ValueArray,
    ByteArray,
    U32Array,
    ModuleRequestArray,
    ModuleOptionArray,
    StackFrameInfoArray,
    FinalizationRegistryCells,
    GlobalScopes,

    // Vectors
    ValueVec,

    // Numerical value is the number of kinds in the enum
    Last,
}

impl HeapItemKind {
    const fn count() -> usize {
        HeapItemKind::Last as usize
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct DescFlags: u8 {
        /// Whether this heap item is an object value
        const IS_OBJECT = 1 << 0;
    }
}

impl HeapItemDescriptor {
    pub fn new<T>(
        cx: Context,
        descriptor: StackRoot<HeapItemDescriptor>,
        kind: HeapItemKind,
        flags: DescFlags,
    ) -> AllocResult<HeapPtr<HeapItemDescriptor>>
    where
        StackRoot<T>: VirtualObject,
    {
        let mut desc = cx.alloc_uninit::<HeapItemDescriptor>()?;

        set_uninit!(desc.descriptor, *descriptor);
        set_uninit!(desc.vtable, extract_virtual_object_vtable::<T>());
        set_uninit!(desc.kind, kind);
        set_uninit!(desc.flags, flags);

        Ok(desc)
    }

    #[inline]
    pub const fn kind(&self) -> HeapItemKind {
        self.kind
    }

    #[inline]
    pub const fn vtable(&self) -> VirtualObjectVtable {
        self.vtable
    }

    #[inline]
    pub fn is_object(&self) -> bool {
        self.flags.contains(DescFlags::IS_OBJECT)
    }

    pub fn byte_size_for_item(&self, item: HeapPtr<AnyHeapItem>) -> usize {
        match self.kind() {
            HeapItemKind::Descriptor => item.cast::<HeapItemDescriptor>().byte_size(),
            HeapItemKind::OrdinaryObject => item.cast::<ObjectValue>().byte_size(),
            HeapItemKind::Proxy => item.cast::<ProxyObject>().byte_size(),
            HeapItemKind::BooleanObject => item.cast::<BooleanObject>().byte_size(),
            HeapItemKind::NumberObject => item.cast::<NumberObject>().byte_size(),
            HeapItemKind::StringObject => item.cast::<StringObject>().byte_size(),
            HeapItemKind::SymbolObject => item.cast::<SymbolObject>().byte_size(),
            HeapItemKind::BigIntObject => item.cast::<BigIntObject>().byte_size(),
            HeapItemKind::ArrayObject => item.cast::<ArrayObject>().byte_size(),
            HeapItemKind::RegExpObject => item.cast::<RegExpObject>().byte_size(),
            HeapItemKind::ErrorObject => item.cast::<ErrorObject>().byte_size(),
            HeapItemKind::DateObject => item.cast::<DateObject>().byte_size(),
            HeapItemKind::SetObject => item.cast::<SetObject>().byte_size(),
            HeapItemKind::MapObject => item.cast::<MapObject>().byte_size(),
            HeapItemKind::WeakRefObject => item.cast::<WeakRefObject>().byte_size(),
            HeapItemKind::WeakSetObject => item.cast::<WeakSetObject>().byte_size(),
            HeapItemKind::WeakMapObject => item.cast::<WeakMapObject>().byte_size(),
            HeapItemKind::FinalizationRegistryObject => {
                item.cast::<FinalizationRegistryObject>().byte_size()
            }
            HeapItemKind::MappedArgumentsObject => item.cast::<MappedArgumentsObject>().byte_size(),
            HeapItemKind::UnmappedArgumentsObject => {
                item.cast::<UnmappedArgumentsObject>().byte_size()
            }
            HeapItemKind::Int8Array => item.cast::<Int8Array>().byte_size(),
            HeapItemKind::UInt8Array => item.cast::<UInt8Array>().byte_size(),
            HeapItemKind::UInt8ClampedArray => item.cast::<UInt8ClampedArray>().byte_size(),
            HeapItemKind::Int16Array => item.cast::<Int16Array>().byte_size(),
            HeapItemKind::UInt16Array => item.cast::<UInt16Array>().byte_size(),
            HeapItemKind::Int32Array => item.cast::<Int32Array>().byte_size(),
            HeapItemKind::UInt32Array => item.cast::<UInt32Array>().byte_size(),
            HeapItemKind::BigInt64Array => item.cast::<BigInt64Array>().byte_size(),
            HeapItemKind::BigUInt64Array => item.cast::<BigUInt64Array>().byte_size(),
            HeapItemKind::Float16Array => item.cast::<Float16Array>().byte_size(),
            HeapItemKind::Float32Array => item.cast::<Float32Array>().byte_size(),
            HeapItemKind::Float64Array => item.cast::<Float64Array>().byte_size(),
            HeapItemKind::ArrayBufferObject => item.cast::<ArrayBufferObject>().byte_size(),
            HeapItemKind::DataViewObject => item.cast::<DataViewObject>().byte_size(),
            HeapItemKind::ArrayIterator => item.cast::<ArrayIterator>().byte_size(),
            HeapItemKind::StringIterator => item.cast::<StringIterator>().byte_size(),
            HeapItemKind::SetIterator => item.cast::<SetIterator>().byte_size(),
            HeapItemKind::MapIterator => item.cast::<MapIterator>().byte_size(),
            HeapItemKind::RegExpStringIterator => item.cast::<RegExpStringIterator>().byte_size(),
            HeapItemKind::ForInIterator => item.cast::<ForInIterator>().byte_size(),
            HeapItemKind::AsyncFromSyncIterator => item.cast::<AsyncFromSyncIterator>().byte_size(),
            HeapItemKind::WrappedValidIterator => item.cast::<WrappedValidIterator>().byte_size(),
            HeapItemKind::IteratorHelperObject => item.cast::<IteratorHelperObject>().byte_size(),
            HeapItemKind::ObjectPrototype => item.cast::<ObjectPrototype>().byte_size(),
            HeapItemKind::String => item.cast::<StringValue>().byte_size(),
            HeapItemKind::Symbol => item.cast::<SymbolValue>().byte_size(),
            HeapItemKind::BigInt => item.cast::<BigIntValue>().byte_size(),
            HeapItemKind::Accessor => item.cast::<Accessor>().byte_size(),
            HeapItemKind::Promise => item.cast::<PromiseObject>().byte_size(),
            HeapItemKind::PromiseReaction => item.cast::<PromiseReaction>().byte_size(),
            HeapItemKind::PromiseCapability => item.cast::<PromiseCapability>().byte_size(),
            HeapItemKind::Realm => item.cast::<Realm>().byte_size(),
            HeapItemKind::Closure => item.cast::<Closure>().byte_size(),
            HeapItemKind::BytecodeFunction => item.cast::<BytecodeFunction>().byte_size(),
            HeapItemKind::ConstantTable => item.cast::<ConstantTable>().byte_size(),
            HeapItemKind::ExceptionStackRootrs => item.cast::<ExceptionStackRootrs>().byte_size(),
            HeapItemKind::SourceFile => item.cast::<SourceFile>().byte_size(),
            HeapItemKind::Scope => item.cast::<Scope>().byte_size(),
            HeapItemKind::ScopeNames => item.cast::<ScopeNames>().byte_size(),
            HeapItemKind::GlobalNames => item.cast::<GlobalNames>().byte_size(),
            HeapItemKind::ClassNames => item.cast::<ClassNames>().byte_size(),
            HeapItemKind::SourceTextModule => item.cast::<SourceTextModule>().byte_size(),
            HeapItemKind::SyntheticModule => item.cast::<SyntheticModule>().byte_size(),
            HeapItemKind::ModuleNamespaceObject => item.cast::<ModuleNamespaceObject>().byte_size(),
            HeapItemKind::ImportAttributes => item.cast::<ImportAttributes>().byte_size(),
            HeapItemKind::Generator => item.cast::<GeneratorObject>().byte_size(),
            HeapItemKind::AsyncGenerator => item.cast::<AsyncGeneratorObject>().byte_size(),
            HeapItemKind::AsyncGeneratorRequest => item.cast::<AsyncGeneratorRequest>().byte_size(),
            HeapItemKind::DenseArrayProperties => item.cast::<DenseArrayProperties>().byte_size(),
            HeapItemKind::SparseArrayProperties => item.cast::<SparseArrayProperties>().byte_size(),
            HeapItemKind::CompiledRegExpObject => item.cast::<CompiledRegExpObject>().byte_size(),
            HeapItemKind::BoxedValue => item.cast::<BoxedValue>().byte_size(),
            HeapItemKind::ObjectNamedPropertiesMap => {
                NamedPropertiesMapField::byte_size(&item.cast())
            }
            HeapItemKind::MapObjectValueMap => MapObjectMapField::byte_size(&item.cast()),
            HeapItemKind::SetObjectValueSet => SetObjectSetField::byte_size(&item.cast()),
            HeapItemKind::ExportMap => ExportMapField::byte_size(&item.cast()),
            HeapItemKind::WeakMapObjectWeakValueMap => {
                WeakMapObjectMapField::byte_size(&item.cast())
            }
            HeapItemKind::WeakSetObjectWeakValueSet => {
                WeakSetObjectSetField::byte_size(&item.cast())
            }
            HeapItemKind::GlobalSymbolRegistryMap => {
                GlobalSymbolRegistryField::byte_size(&item.cast())
            }
            HeapItemKind::InternedStringsSet => InternedStringsSetField::byte_size(&item.cast()),
            HeapItemKind::LexicalNamesMap => LexicalNamesMapField::byte_size(&item.cast()),
            HeapItemKind::ModuleCacheMap => ModuleCacheField::byte_size(&item.cast()),
            HeapItemKind::ValueArray => value_array_byte_size(item.cast()),
            HeapItemKind::ByteArray => byte_array_byte_size(item.cast()),
            HeapItemKind::U32Array => u32_array_byte_size(item.cast()),
            HeapItemKind::ModuleRequestArray => module_request_array_byte_size(item.cast()),
            HeapItemKind::ModuleOptionArray => module_option_array_byte_size(item.cast()),
            HeapItemKind::StackFrameInfoArray => stack_frame_info_array_byte_size(item.cast()),
            HeapItemKind::FinalizationRegistryCells => {
                item.cast::<FinalizationRegistryCells>().byte_size()
            }
            HeapItemKind::GlobalScopes => item.cast::<GlobalScopes>().byte_size(),
            HeapItemKind::ValueVec => value_vec_byte_size(item.cast()),
            HeapItemKind::Last => unreachable!("No objects are created with this descriptor"),
        }
    }
}

pub struct BaseDescriptors {
    descriptors: Vec<HeapPtr<HeapItemDescriptor>>,
}

impl BaseDescriptors {
    pub fn uninit_empty() -> Self {
        BaseDescriptors {
            descriptors: vec![],
        }
    }

    pub fn uninit() -> Self {
        let mut descriptors = vec![];

        descriptors.reserve_exact(HeapItemKind::count());
        unsafe { descriptors.set_len(HeapItemKind::count()) };

        BaseDescriptors { descriptors }
    }

    pub fn new(cx: Context) -> AllocResult<BaseDescriptors> {
        let mut base_descriptors = Self::uninit();
        let descriptors = &mut base_descriptors.descriptors;

        // Create fake handle which will be read from, in order to initialize descriptor descriptor
        let value = Value::empty();
        let fake_descriptor_handle = StackRoot::<Value>::from_fixed_non_heap_ptr(&value).cast();

        // First set up the singleton descriptor descriptor, using an arbitrary vtable
        // (e.g. OrdinaryObject). Can only set self pointer after object initially created.
        let mut descriptor = HeapItemDescriptor::new::<OrdinaryObject>(
            cx,
            fake_descriptor_handle,
            HeapItemKind::Descriptor,
            DescFlags::empty(),
        )?
        .to_stack(cx);
        descriptor.descriptor = *descriptor;
        descriptors[HeapItemKind::Descriptor as usize] = *descriptor;

        macro_rules! register_descriptor {
            ($object_kind:expr, $object_ty:ty, $flags:expr) => {
                let desc =
                    HeapItemDescriptor::new::<$object_ty>(cx, descriptor, $object_kind, $flags)?;
                descriptors[$object_kind as usize] = desc;
            };
        }

        macro_rules! ordinary_object_descriptor {
            ($object_kind:expr) => {
                register_descriptor!($object_kind, OrdinaryObject, DescFlags::IS_OBJECT);
            };
        }

        macro_rules! other_heap_item_descriptor {
            ($object_kind:expr) => {
                register_descriptor!($object_kind, OrdinaryObject, DescFlags::empty());
            };
        }

        ordinary_object_descriptor!(HeapItemKind::OrdinaryObject);
        register_descriptor!(HeapItemKind::Proxy, ProxyObject, DescFlags::IS_OBJECT);

        ordinary_object_descriptor!(HeapItemKind::BooleanObject);
        ordinary_object_descriptor!(HeapItemKind::NumberObject);
        register_descriptor!(
            HeapItemKind::StringObject,
            StringObject,
            DescFlags::IS_OBJECT
        );
        ordinary_object_descriptor!(HeapItemKind::SymbolObject);
        ordinary_object_descriptor!(HeapItemKind::BigIntObject);
        register_descriptor!(HeapItemKind::ArrayObject, ArrayObject, DescFlags::IS_OBJECT);
        ordinary_object_descriptor!(HeapItemKind::RegExpObject);
        ordinary_object_descriptor!(HeapItemKind::ErrorObject);
        ordinary_object_descriptor!(HeapItemKind::DateObject);
        ordinary_object_descriptor!(HeapItemKind::SetObject);
        ordinary_object_descriptor!(HeapItemKind::MapObject);
        ordinary_object_descriptor!(HeapItemKind::WeakRefObject);
        ordinary_object_descriptor!(HeapItemKind::WeakSetObject);
        ordinary_object_descriptor!(HeapItemKind::WeakMapObject);
        ordinary_object_descriptor!(HeapItemKind::FinalizationRegistryObject);

        register_descriptor!(
            HeapItemKind::MappedArgumentsObject,
            MappedArgumentsObject,
            DescFlags::IS_OBJECT
        );
        ordinary_object_descriptor!(HeapItemKind::UnmappedArgumentsObject);

        register_descriptor!(HeapItemKind::Int8Array, Int8Array, DescFlags::IS_OBJECT);
        register_descriptor!(HeapItemKind::UInt8Array, UInt8Array, DescFlags::IS_OBJECT);
        register_descriptor!(
            HeapItemKind::UInt8ClampedArray,
            UInt8ClampedArray,
            DescFlags::IS_OBJECT
        );
        register_descriptor!(HeapItemKind::Int16Array, Int16Array, DescFlags::IS_OBJECT);
        register_descriptor!(HeapItemKind::UInt16Array, UInt16Array, DescFlags::IS_OBJECT);
        register_descriptor!(HeapItemKind::Int32Array, Int32Array, DescFlags::IS_OBJECT);
        register_descriptor!(HeapItemKind::UInt32Array, UInt32Array, DescFlags::IS_OBJECT);
        register_descriptor!(
            HeapItemKind::BigInt64Array,
            BigInt64Array,
            DescFlags::IS_OBJECT
        );
        register_descriptor!(
            HeapItemKind::BigUInt64Array,
            BigUInt64Array,
            DescFlags::IS_OBJECT
        );
        register_descriptor!(
            HeapItemKind::Float16Array,
            Float16Array,
            DescFlags::IS_OBJECT
        );
        register_descriptor!(
            HeapItemKind::Float32Array,
            Float32Array,
            DescFlags::IS_OBJECT
        );
        register_descriptor!(
            HeapItemKind::Float64Array,
            Float64Array,
            DescFlags::IS_OBJECT
        );

        ordinary_object_descriptor!(HeapItemKind::ArrayBufferObject);
        ordinary_object_descriptor!(HeapItemKind::DataViewObject);

        ordinary_object_descriptor!(HeapItemKind::ArrayIterator);
        ordinary_object_descriptor!(HeapItemKind::StringIterator);
        ordinary_object_descriptor!(HeapItemKind::SetIterator);
        ordinary_object_descriptor!(HeapItemKind::MapIterator);
        ordinary_object_descriptor!(HeapItemKind::RegExpStringIterator);
        other_heap_item_descriptor!(HeapItemKind::ForInIterator);
        ordinary_object_descriptor!(HeapItemKind::AsyncFromSyncIterator);
        ordinary_object_descriptor!(HeapItemKind::WrappedValidIterator);
        ordinary_object_descriptor!(HeapItemKind::IteratorHelperObject);

        ordinary_object_descriptor!(HeapItemKind::ObjectPrototype);

        other_heap_item_descriptor!(HeapItemKind::String);
        other_heap_item_descriptor!(HeapItemKind::Symbol);
        other_heap_item_descriptor!(HeapItemKind::BigInt);
        other_heap_item_descriptor!(HeapItemKind::Accessor);

        ordinary_object_descriptor!(HeapItemKind::Promise);
        other_heap_item_descriptor!(HeapItemKind::PromiseReaction);
        other_heap_item_descriptor!(HeapItemKind::PromiseCapability);

        other_heap_item_descriptor!(HeapItemKind::Realm);

        ordinary_object_descriptor!(HeapItemKind::Closure);
        other_heap_item_descriptor!(HeapItemKind::BytecodeFunction);
        other_heap_item_descriptor!(HeapItemKind::ConstantTable);
        other_heap_item_descriptor!(HeapItemKind::ExceptionStackRootrs);
        other_heap_item_descriptor!(HeapItemKind::SourceFile);

        other_heap_item_descriptor!(HeapItemKind::Scope);
        other_heap_item_descriptor!(HeapItemKind::ScopeNames);
        other_heap_item_descriptor!(HeapItemKind::GlobalNames);
        other_heap_item_descriptor!(HeapItemKind::ClassNames);

        other_heap_item_descriptor!(HeapItemKind::SourceTextModule);
        other_heap_item_descriptor!(HeapItemKind::SyntheticModule);
        register_descriptor!(
            HeapItemKind::ModuleNamespaceObject,
            ModuleNamespaceObject,
            DescFlags::IS_OBJECT
        );
        other_heap_item_descriptor!(HeapItemKind::ImportAttributes);

        ordinary_object_descriptor!(HeapItemKind::Generator);
        ordinary_object_descriptor!(HeapItemKind::AsyncGenerator);
        other_heap_item_descriptor!(HeapItemKind::AsyncGeneratorRequest);

        other_heap_item_descriptor!(HeapItemKind::DenseArrayProperties);
        other_heap_item_descriptor!(HeapItemKind::SparseArrayProperties);

        other_heap_item_descriptor!(HeapItemKind::CompiledRegExpObject);

        other_heap_item_descriptor!(HeapItemKind::BoxedValue);

        other_heap_item_descriptor!(HeapItemKind::ObjectNamedPropertiesMap);
        other_heap_item_descriptor!(HeapItemKind::MapObjectValueMap);
        other_heap_item_descriptor!(HeapItemKind::SetObjectValueSet);
        other_heap_item_descriptor!(HeapItemKind::ExportMap);
        other_heap_item_descriptor!(HeapItemKind::WeakMapObjectWeakValueMap);
        other_heap_item_descriptor!(HeapItemKind::WeakSetObjectWeakValueSet);
        other_heap_item_descriptor!(HeapItemKind::GlobalSymbolRegistryMap);
        other_heap_item_descriptor!(HeapItemKind::InternedStringsSet);
        other_heap_item_descriptor!(HeapItemKind::LexicalNamesMap);
        other_heap_item_descriptor!(HeapItemKind::ModuleCacheMap);

        other_heap_item_descriptor!(HeapItemKind::ValueArray);
        other_heap_item_descriptor!(HeapItemKind::ByteArray);
        other_heap_item_descriptor!(HeapItemKind::U32Array);
        other_heap_item_descriptor!(HeapItemKind::ModuleRequestArray);
        other_heap_item_descriptor!(HeapItemKind::ModuleOptionArray);
        other_heap_item_descriptor!(HeapItemKind::StackFrameInfoArray);
        other_heap_item_descriptor!(HeapItemKind::FinalizationRegistryCells);
        other_heap_item_descriptor!(HeapItemKind::GlobalScopes);

        other_heap_item_descriptor!(HeapItemKind::ValueVec);

        Ok(base_descriptors)
    }

    pub fn get(&self, kind: HeapItemKind) -> HeapPtr<HeapItemDescriptor> {
        self.descriptors[kind as usize]
    }

    pub fn visit_roots(&mut self, visitor: &mut impl GcVisitorExt) {
        for descriptor in &mut self.descriptors {
            visitor.visit_pointer(descriptor);
        }
    }
}

impl HeapItem for HeapPtr<HeapItemDescriptor> {
    fn byte_size(&self) -> usize {
        size_of::<HeapItemDescriptor>()
    }

    fn visit_pointers(&mut self, visitor: &mut impl GcVisitorExt) {
        visitor.visit_pointer(&mut self.descriptor);
        visitor.visit_rust_vtable_pointer(&mut self.vtable);
    }
}
