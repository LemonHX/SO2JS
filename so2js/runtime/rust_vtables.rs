use super::{
    arguments_object::MappedArgumentsObject,
    array_object::ArrayObject,
    intrinsics::typed_array::{
        BigInt64Array, BigUInt64Array, Float16Array, Float32Array, Float64Array, Int16Array,
        Int32Array, Int8Array, TypedArray, UInt16Array, UInt32Array, UInt8Array, UInt8ClampedArray,
    },
    module::{
        module::Module, module_namespace_object::ModuleNamespaceObject,
        source_text_module::SourceTextModule, synthetic_module::SyntheticModule,
    },
    object_value::VirtualObject,
    ordinary_object::OrdinaryObject,
    proxy_object::ProxyObject,
    string_object::StringObject,
};
use alloc::vec::Vec;
use once_cell::sync::Lazy;

/// Every Rust trait vtable that can appear in the heap.
///
/// This occurs due to the implementation of dynamic dispatch where the corresponding Rust trait
/// vtable is stored in the heap and used to reconstruct a Rust trait object.
#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum RustVtable {
    // VirtualObjects
    OrdinaryVirtualObject,
    ArrayVirtualObject,
    StringVirtualObject,
    ProxyVirtualObject,
    ModuleNamespaceVirtualObject,
    MappedArgumentsVirtualObject,
    Int8ArrayVirtualObject,
    UInt8ArrayVirtualObject,
    UInt8ClampedArrayVirtualObject,
    Int16ArrayVirtualObject,
    UInt16ArrayVirtualObject,
    Int32ArrayVirtualObject,
    Uint32ArrayVirtualObject,
    Float16ArrayVirtualObject,
    Float32ArrayVirtualObject,
    Float64ArrayVirtualObject,
    BigInt64ArrayVirtualObject,
    BigUInt64ArrayVirtualObject,
    // Modules
    SourceTextModule,
    SyntheticModule,
    // TypedArrays
    Int8TypedArray,
    UInt8TypedArray,
    UInt8ClampedTypedArray,
    Int16TypedArray,
    UInt16TypedArray,
    Int32TypedArray,
    UInt32TypedArray,
    Float16TypedArray,
    Float32TypedArray,
    Float64TypedArray,
    BigInt64TypedArray,
    BigUInt64TypedArray,
    // Last entry for the number of vtables registered
    Last,
}

/// The vtables stored in order. Can be indexed with the RustVtable enum to find the vtable pointer
/// of a particular type.
const RUST_VTABLES: [*const (); RustVtable::Last as usize] = [
    // VirtualObjects
    OrdinaryObject::VIRTUAL_OBJECT_VTABLE,
    ArrayObject::VIRTUAL_OBJECT_VTABLE,
    StringObject::VIRTUAL_OBJECT_VTABLE,
    ProxyObject::VIRTUAL_OBJECT_VTABLE,
    ModuleNamespaceObject::VIRTUAL_OBJECT_VTABLE,
    MappedArgumentsObject::VIRTUAL_OBJECT_VTABLE,
    Int8Array::VIRTUAL_OBJECT_VTABLE,
    UInt8Array::VIRTUAL_OBJECT_VTABLE,
    UInt8ClampedArray::VIRTUAL_OBJECT_VTABLE,
    Int16Array::VIRTUAL_OBJECT_VTABLE,
    UInt16Array::VIRTUAL_OBJECT_VTABLE,
    Int32Array::VIRTUAL_OBJECT_VTABLE,
    UInt32Array::VIRTUAL_OBJECT_VTABLE,
    Float16Array::VIRTUAL_OBJECT_VTABLE,
    Float32Array::VIRTUAL_OBJECT_VTABLE,
    Float64Array::VIRTUAL_OBJECT_VTABLE,
    BigInt64Array::VIRTUAL_OBJECT_VTABLE,
    BigUInt64Array::VIRTUAL_OBJECT_VTABLE,
    // Modules
    SourceTextModule::MODULE_VTABLE,
    SyntheticModule::MODULE_VTABLE,
    // TypedArrays
    Int8Array::TYPED_ARRAY_VTABLE,
    UInt8Array::TYPED_ARRAY_VTABLE,
    UInt8ClampedArray::TYPED_ARRAY_VTABLE,
    Int16Array::TYPED_ARRAY_VTABLE,
    UInt16Array::TYPED_ARRAY_VTABLE,
    Int32Array::TYPED_ARRAY_VTABLE,
    UInt32Array::TYPED_ARRAY_VTABLE,
    Float16Array::TYPED_ARRAY_VTABLE,
    Float32Array::TYPED_ARRAY_VTABLE,
    Float64Array::TYPED_ARRAY_VTABLE,
    BigInt64Array::TYPED_ARRAY_VTABLE,
    BigUInt64Array::TYPED_ARRAY_VTABLE,
];

pub struct VtablePtr(*const (), RustVtable);
unsafe impl Send for VtablePtr {}
unsafe impl Sync for VtablePtr {}

/// Array of (vtable, enum) pairs sorted by vtable pointer. This is used to look up the enum for a
/// particular vtable pointer in O(log n) time.
pub static RUST_VTABLES_SORTED_BY_POINTER: Lazy<Vec<VtablePtr>> = Lazy::new(|| {
    let mut vtables = Vec::with_capacity(RustVtable::Last as usize);

    for (i, &vtable) in RUST_VTABLES.iter().enumerate() {
        let enum_value = unsafe { core::mem::transmute::<u8, RustVtable>(i as u8) };
        vtables.push(VtablePtr(vtable, enum_value));
    }

    vtables.sort_by_key(|&VtablePtr(ptr, _)| ptr);

    vtables
});

/// Return the vtable pointer for a particular type.
#[allow(unused)]
pub const fn get_vtable(vtable: RustVtable) -> *const () {
    RUST_VTABLES[vtable as usize]
}

/// Lookup the RustVtable enum value for a particular vtable pointer. If the vtable pointer is not
/// registered then return None.
#[allow(unused)]
pub fn lookup_vtable_enum(vtable: *const ()) -> Option<RustVtable> {
    let index = RUST_VTABLES_SORTED_BY_POINTER
        .binary_search_by_key(&vtable, |&VtablePtr(ptr, _)| ptr)
        .ok()?;

    Some(RUST_VTABLES_SORTED_BY_POINTER[index].1)
}

/// Rust representation of a fat pointer. Used to extract vtable pointer.
#[repr(C)]
struct FatPointer {
    data: *const (),
    vtable: *const (),
}

/// Compile time shenanigans to extract the trait object vtable for a particular type that
/// implements the trait so that we can construct our own trait objects manually.
macro_rules! extract_vtable_function {
    ($method_name:ident, $trait:ident) => {
        pub const fn $method_name<T>() -> *const ()
        where
            $crate::runtime::StackRoot<T>: $trait,
        {
            unsafe {
                let example_ptr: *const $crate::runtime::StackRoot<T> =
                    core::ptr::NonNull::dangling().as_ptr();
                let example_fat_ptr: *const dyn $trait = example_ptr;
                let fat_ptr =
                    core::mem::transmute::<*const dyn $trait, FatPointer>(example_fat_ptr);

                fat_ptr.vtable
            }
        }
    };
}

extract_vtable_function!(extract_virtual_object_vtable, VirtualObject);
extract_vtable_function!(extract_module_vtable, Module);
extract_vtable_function!(extract_typed_array_vtable, TypedArray);
