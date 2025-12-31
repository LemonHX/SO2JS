#[macro_export]
macro_rules! heap_trait_object {
    ($trait:ident, $stack_object:ident, $heap_object:ident, $into_dyn:ident, $extract_vtable:ident) => {
        /// A custom trait object to the heap, containing both a pointer to an object on the heap along with
        /// the object's vtable for the trait.
        ///
        /// Differs from a true rust trait object in that the data pointer contains the receiver value
        /// directly instead of a pointer to the receiver.
        #[derive(Clone, Copy)]
        #[repr(C)]
        pub struct $stack_object {
            pub data: $crate::runtime::StackRoot<$crate::runtime::object_value::ObjectValue>,
            vtable: *const (),
        }

        /// The same custom trait object, but stored on the heap.
        #[derive(Clone, Copy)]
        #[repr(C)]
        pub struct $heap_object {
            data: $crate::runtime::HeapPtr<$crate::runtime::object_value::ObjectValue>,
            vtable: *const (),
        }

        impl<T> $crate::runtime::StackRoot<T>
        where
            $crate::runtime::StackRoot<T>: $trait,
        {
            #[inline]
            pub fn $into_dyn(self) -> $stack_object
            where
                Self: Sized,
            {
                let vtable = $extract_vtable();
                $stack_object {
                    data: self.cast(),
                    vtable,
                }
            }
        }

        impl $heap_object {
            #[allow(dead_code)]
            pub fn uninit() -> $heap_object {
                $heap_object {
                    data: $crate::runtime::HeapPtr::uninit(),
                    vtable: core::ptr::null(),
                }
            }

            #[allow(dead_code)]
            pub fn visit_pointers(&mut self, visitor: &mut impl $crate::runtime::gc::GcVisitorExt) {
                visitor.visit_pointer(&mut self.data);
                visitor.visit_rust_vtable_pointer(&mut self.vtable);
            }
        }

        impl $stack_object {
            #[allow(dead_code)]
            pub fn ptr_eq(&self, other: &Self) -> bool {
                (*self.data).ptr_eq(&*other.data)
            }

            #[allow(dead_code)]
            #[inline]
            pub fn to_heap(self) -> $heap_object {
                $heap_object {
                    data: *self.data,
                    vtable: self.vtable,
                }
            }

            #[allow(dead_code)]
            #[inline]
            pub fn from_heap(
                cx: $crate::runtime::Context,
                heap_object: &$heap_object,
            ) -> $stack_object {
                $stack_object {
                    data: heap_object.data.to_stack(cx),
                    vtable: heap_object.vtable,
                }
            }
        }

        #[repr(C)]
        struct RustTraitObject {
            data: *const (),
            vtable: *const (),
        }

        // Implicitly deref to a true rust trait object by constructing a true trait object with a pointer
        // to the receiver value, with the same vtable.
        impl core::ops::Deref for $stack_object {
            type Target = dyn $trait;

            fn deref(&self) -> &Self::Target {
                let data = &self.data as *const _ as *const ();
                let trait_object = RustTraitObject {
                    data,
                    vtable: self.vtable,
                };
                unsafe { core::mem::transmute::<RustTraitObject, &dyn $trait>(trait_object) }
            }
        }

        impl core::ops::DerefMut for $stack_object {
            fn deref_mut(&mut self) -> &mut Self::Target {
                let data = &self.data as *const _ as *const ();
                let trait_object = RustTraitObject {
                    data,
                    vtable: self.vtable,
                };
                unsafe { core::mem::transmute::<RustTraitObject, &mut dyn $trait>(trait_object) }
            }
        }
    };
}
