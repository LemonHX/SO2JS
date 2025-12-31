use crate::{
    completion_value, eval_err, if_abrupt_reject_promise, must, must_a,
    runtime::{
        abstract_operations::{call_object, enumerable_own_property_names, KeyOrValue},
        alloc_error::AllocResult,
        builtin_function::BuiltinFunction,
        context::ModuleCacheKey,
        error::type_error_value,
        function::get_argument,
        get,
        heap_item_descriptor::HeapItemKind,
        interned_strings::InternedStrings,
        intrinsics::{
            intrinsics::Intrinsic, promise_prototype::perform_promise_then,
            rust_runtime::RustRuntimeFunction,
        },
        module::{
            module::{DynModule, ModuleEnum},
            synthetic_module::SyntheticModule,
        },
        object_value::ObjectValue,
        promise_object::{PromiseCapability, PromiseObject},
        string_value::FlatString,
        to_string, Context, EvalResult, PropertyKey, StackRoot, Value,
    },
};
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use hashbrown::HashMap;

use super::{
    import_attributes::ImportAttributes,
    module::{Module, ModuleId},
    source_text_module::{ModuleRequest, ModuleState, SourceTextModule},
};

/// Execute a module - loading, linking, and evaluating it and its dependencies.
///
/// Returns a promise that resolves once the module has completed execution.
pub fn execute_module(
    mut cx: Context,
    module: StackRoot<SourceTextModule>,
) -> AllocResult<StackRoot<PromiseObject>> {
    let promise_constructor = cx.get_intrinsic(Intrinsic::PromiseConstructor);
    let capability = must_a!(PromiseCapability::new(cx, promise_constructor.into()));

    match &cx.sys {
        Some(sys) => {
            // Cache the module at its canonical source path
            let source_file_path = sys.path_canonicalize(&module.source_file_path().to_string());

            // Modules executing directly are assumed to have no attributes
            let module_cache_key = ModuleCacheKey::new(source_file_path, None);
            cx.insert_module(module_cache_key, module.as_dyn_module())?;

            let promise = module.load_requested_modules(cx)?;

            let on_resolve = callback(cx, load_requested_modules_static_resolve)?;
            set_module(cx, on_resolve, module)?;
            set_capability(cx, on_resolve, capability)?;

            let on_reject = callback(cx, load_requested_modules_reject)?;
            set_capability(cx, on_reject, capability)?;

            perform_promise_then(cx, promise, on_resolve.into(), on_reject.into(), None)?;

            // Guaranteed to be a PromiseObject since created with the Promise constructor
            Ok(capability.promise(cx).cast::<PromiseObject>())
        }
        None => {
            todo!()
        }
    }
}

fn get_module(cx: Context, function: StackRoot<ObjectValue>) -> StackRoot<SourceTextModule> {
    function
        .private_element_find(cx, cx.well_known_symbols.module().cast())
        .unwrap()
        .value()
        .as_object()
        .cast::<SourceTextModule>()
}

fn set_module(
    cx: Context,
    mut function: StackRoot<ObjectValue>,
    value: StackRoot<SourceTextModule>,
) -> AllocResult<()> {
    function.private_element_set(cx, cx.well_known_symbols.module().cast(), value.into())
}

fn get_dyn_module(cx: Context, function: StackRoot<ObjectValue>) -> DynModule {
    let item = function
        .private_element_find(cx, cx.well_known_symbols.module().cast())
        .unwrap()
        .value()
        .as_pointer();

    debug_assert!(
        item.descriptor().kind() == HeapItemKind::SourceTextModule
            || item.descriptor().kind() == HeapItemKind::SyntheticModule
    );

    if item.descriptor().kind() == HeapItemKind::SourceTextModule {
        item.cast::<SourceTextModule>().to_stack(cx).as_dyn_module()
    } else {
        item.cast::<SyntheticModule>().to_stack(cx).as_dyn_module()
    }
}

fn set_dyn_module(
    cx: Context,
    mut function: StackRoot<ObjectValue>,
    value: DynModule,
) -> AllocResult<()> {
    function.private_element_set(
        cx,
        cx.well_known_symbols.module().cast(),
        value.as_heap_item().into(),
    )
}

fn get_capability(cx: Context, function: StackRoot<ObjectValue>) -> StackRoot<PromiseCapability> {
    function
        .private_element_find(cx, cx.well_known_symbols.capability().cast())
        .unwrap()
        .value()
        .as_object()
        .cast::<PromiseCapability>()
}

fn set_capability(
    cx: Context,
    mut function: StackRoot<ObjectValue>,
    value: StackRoot<PromiseCapability>,
) -> AllocResult<()> {
    function.private_element_set(cx, cx.well_known_symbols.capability().cast(), value.into())
}

pub fn load_requested_modules_static_resolve(
    mut cx: Context,
    _: StackRoot<Value>,
    _: &[StackRoot<Value>],
) -> EvalResult<StackRoot<Value>> {
    // Fetch the module and capbility passed from `execute_module`
    let current_function = cx.current_function();
    let module = get_module(cx, current_function);
    let capability = get_capability(cx, current_function);

    if let Err(error) = completion_value!(module.link(cx)) {
        must!(call_object(
            cx,
            capability.reject(cx),
            cx.undefined(),
            &[error]
        ));
        return Ok(cx.undefined());
    }

    // Mark the module resolution phase as complete
    cx.has_finished_module_resolution = true;
    cx.vm().mark_stack_trace_top();

    let evaluate_promise = module.evaluate(cx)?;

    Ok(perform_promise_then(
        cx,
        evaluate_promise,
        cx.undefined(),
        cx.undefined(),
        Some(capability),
    )?)
}

pub fn load_requested_modules_reject(
    mut cx: Context,
    _: StackRoot<Value>,
    arguments: &[StackRoot<Value>],
) -> EvalResult<StackRoot<Value>> {
    // Fetch the capbility passed from `execute_module`
    let current_function = cx.current_function();
    let capability = get_capability(cx, current_function);

    let error = get_argument(cx, arguments, 0);
    must!(call_object(
        cx,
        capability.reject(cx),
        cx.undefined(),
        &[error]
    ));

    Ok(cx.undefined())
}

fn callback(cx: Context, func: RustRuntimeFunction) -> AllocResult<StackRoot<ObjectValue>> {
    let realm = cx.current_realm();
    Ok(BuiltinFunction::create_builtin_function_without_properties(
        cx,
        func,
        /* name */ None,
        realm,
        /* prototype */ Some(realm.get_intrinsic(Intrinsic::FunctionPrototype)),
        /* is_constructor */ false,
    )?
    .into())
}

pub fn module_evaluate(
    cx: Context,
    module: StackRoot<SourceTextModule>,
) -> AllocResult<StackRoot<PromiseObject>> {
    let mut evaluator = GraphEvaluator::new();
    evaluator.evaluate(cx, module)
}

struct GraphEvaluator {
    stack: Vec<StackRoot<SourceTextModule>>,
}

impl GraphEvaluator {
    fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Evaluate (https://tc39.es/ecma262/#sec-moduleevaluation)
    fn evaluate(
        &mut self,
        cx: Context,
        mut module: StackRoot<SourceTextModule>,
    ) -> AllocResult<StackRoot<PromiseObject>> {
        if matches!(
            module.state(),
            ModuleState::Evaluated | ModuleState::EvaluatingAsync
        ) {
            module = module.cycle_root().unwrap();
        } else {
            debug_assert!(module.state() == ModuleState::Linked);
        }

        if let Some(capability) = module.top_level_capability_ptr() {
            // Was created with promise constructor
            return Ok(capability.promise(cx).cast::<PromiseObject>());
        }

        let promise_constructor = cx.get_intrinsic(Intrinsic::PromiseConstructor);
        let capability = must_a!(PromiseCapability::new(cx, promise_constructor.into()));
        module.set_top_level_capability(*capability);

        let evaluation_result = self.inner_evaluate(cx, module.as_dyn_module(), 0);

        match completion_value!(evaluation_result) {
            Ok(_) => {
                debug_assert!(matches!(
                    module.state(),
                    ModuleState::Evaluated | ModuleState::EvaluatingAsync
                ));
                debug_assert!(module.evaluation_error_ptr().is_none());

                if !module.is_async_evaluation() {
                    debug_assert!(module.state() == ModuleState::Evaluated);
                    must_a!(call_object(
                        cx,
                        capability.resolve(cx),
                        cx.undefined(),
                        &[cx.undefined()]
                    ));
                }

                debug_assert!(self.stack.is_empty());
            }
            Err(error) => {
                for module in &mut self.stack {
                    debug_assert!(module.state() == ModuleState::Evaluating);
                    module.set_state(ModuleState::Evaluated);
                    module.set_evaluation_error(*error);
                }

                debug_assert!(module.state() == ModuleState::Evaluated);

                must_a!(call_object(
                    cx,
                    capability.reject(cx),
                    cx.undefined(),
                    &[error]
                ));
            }
        }

        // Known to be a PromiseObject since created with the Promise constructor
        Ok(capability.promise(cx).cast::<PromiseObject>())
    }

    /// InnerModuleEvaluation (https://tc39.es/ecma262/#sec-innermoduleevaluation)
    fn inner_evaluate(
        &mut self,
        mut cx: Context,
        module: DynModule,
        index: u32,
    ) -> EvalResult<u32> {
        let mut module = match module.as_enum() {
            // Directly evaluate synthetic modules
            ModuleEnum::Synthetic(module) => {
                let promise = module.evaluate(cx)?;

                // Propagate rejected value as error
                return if let Some(rejected_value) = promise.rejected_value() {
                    eval_err!(rejected_value.to_stack(cx))
                } else {
                    Ok(index)
                };
            }
            ModuleEnum::SourceText(module) => module,
        };

        if matches!(
            module.state(),
            ModuleState::Evaluated | ModuleState::EvaluatingAsync
        ) {
            if let Some(error) = module.evaluation_error(cx) {
                return eval_err!(error);
            } else {
                return Ok(index);
            }
        }

        if module.state() == ModuleState::Evaluating {
            return Ok(index);
        }

        debug_assert!(module.state() == ModuleState::Linked);

        // Note that the value of [[PendingAsyncDependencies]] is already 0
        module.set_state(ModuleState::Evaluating);
        module.set_dfs_index(index);
        module.set_dfs_ancestor_index(index);

        self.stack.push(module);

        let mut index = index + 1;

        let loaded_modules = module.loaded_modules();
        for i in 0..loaded_modules.len() {
            let required_module = DynModule::from_heap(cx, &loaded_modules.as_slice()[i].unwrap());

            index = self.inner_evaluate(cx, required_module, index)?;

            if let Some(mut required_module) = required_module.as_source_text_module() {
                if required_module.state() == ModuleState::Evaluating {
                    let new_index = module
                        .dfs_ancestor_index()
                        .min(required_module.dfs_ancestor_index());
                    module.set_dfs_ancestor_index(new_index)
                } else {
                    debug_assert!(matches!(
                        required_module.state(),
                        ModuleState::EvaluatingAsync | ModuleState::Evaluated
                    ));

                    required_module = required_module.cycle_root().unwrap();

                    debug_assert!(matches!(
                        required_module.state(),
                        ModuleState::EvaluatingAsync | ModuleState::Evaluated
                    ));

                    if let Some(error) = required_module.evaluation_error(cx) {
                        return eval_err!(error);
                    }
                }

                if required_module.is_async_evaluation() {
                    module.inc_pending_async_dependencies();
                    required_module.push_async_parent_module(cx, module)?;
                }
            }
        }

        if module.pending_async_dependencies() > 0 || module.has_top_level_await() {
            debug_assert!(!module.is_async_evaluation());
            module.set_async_evaluation(cx, true);

            if module.pending_async_dependencies() == 0 {
                execute_async_module(cx, module)?;
            }
        } else {
            cx.vm().execute_module(module, &[])?;
        }

        debug_assert!(module.dfs_ancestor_index() <= module.dfs_index());

        if module.dfs_ancestor_index() == module.dfs_index() {
            loop {
                let mut required_module = self.stack.pop().unwrap();

                if !required_module.is_async_evaluation() {
                    required_module.set_state(ModuleState::Evaluated);
                } else {
                    required_module.set_state(ModuleState::EvaluatingAsync);
                }

                required_module.set_cycle_root(*module);

                if required_module.ptr_eq(&module) {
                    break;
                }
            }
        }

        Ok(index)
    }
}

/// ExecuteAsyncModule (https://tc39.es/ecma262/#sec-execute-async-module)
fn execute_async_module(mut cx: Context, module: StackRoot<SourceTextModule>) -> AllocResult<()> {
    debug_assert!(matches!(
        module.state(),
        ModuleState::Evaluating | ModuleState::EvaluatingAsync
    ));
    debug_assert!(module.has_top_level_await());

    let promise_constructor = cx.get_intrinsic(Intrinsic::PromiseConstructor);
    let capability = must_a!(PromiseCapability::new(cx, promise_constructor.into()));

    // Known to be a PromiseObject since it was created by the intrinsic Promise constructor
    let promise = capability.promise(cx).cast::<PromiseObject>();

    let on_resolve = callback(cx, async_module_execution_fulfilled)?;
    set_module(cx, on_resolve, module)?;

    let on_reject = callback(cx, async_module_execution_rejected_runtime)?;
    set_module(cx, on_reject, module)?;

    // Set up resolve and reject callbacks which re-enter module graph evaluation
    perform_promise_then(cx, promise, on_resolve.into(), on_reject.into(), None)?;

    // Finally call the module function itself, which will resolve or reject the promise
    must_a!(cx.vm().execute_module(module, &[promise.into()]));

    Ok(())
}

/// AsyncModuleExecutionFulfilled (https://tc39.es/ecma262/#sec-async-module-execution-fulfilled)
pub fn async_module_execution_fulfilled(
    mut cx: Context,
    _: StackRoot<Value>,
    _: &[StackRoot<Value>],
) -> EvalResult<StackRoot<Value>> {
    // Fetch the module passed from `execute_async_module`
    let current_function = cx.current_function();
    let mut module = get_module(cx, current_function);

    if module.state() == ModuleState::Evaluated {
        debug_assert!(module.evaluation_error_ptr().is_some());
        return Ok(cx.undefined());
    }

    debug_assert!(module.state() == ModuleState::EvaluatingAsync);
    debug_assert!(module.is_async_evaluation());
    debug_assert!(module.evaluation_error_ptr().is_none());

    // Mark evaluation of module as complete
    module.set_async_evaluation(cx, false);
    module.set_state(ModuleState::Evaluated);

    // If an entire cycle has been completed, resolve the top-level capability for the cycle
    if let Some(capability) = module.top_level_capability_ptr() {
        debug_assert!(module.cycle_root_ptr().unwrap().ptr_eq(&module));
        must!(call_object(
            cx,
            capability.resolve(cx),
            cx.undefined(),
            &[cx.undefined()]
        ));
    }

    // Gather available ancestors
    let mut ancestors = HashMap::new();
    gather_available_ancestors(cx, module, &mut ancestors);

    // Sort ancestors by the order [[AsyncEvaluation]] was set
    let mut ancestors = ancestors.into_values().collect::<Vec<_>>();
    ancestors.sort_by_key(|module| module.async_evaluation_index().unwrap());

    for mut ancestor in ancestors {
        if ancestor.state() == ModuleState::Evaluated {
            debug_assert!(ancestor.evaluation_error_ptr().is_some());
            continue;
        }

        if ancestor.has_top_level_await() {
            execute_async_module(cx, ancestor)?;
            continue;
        }

        let execute_result = cx.vm().execute_module(ancestor, &[]);

        if let Err(error) = completion_value!(execute_result) {
            async_module_execution_rejected(cx, ancestor, error)?;
            continue;
        }

        ancestor.set_async_evaluation(cx, false);
        ancestor.set_state(ModuleState::Evaluated);

        if let Some(capability) = ancestor.top_level_capability_ptr() {
            debug_assert!(ancestor.cycle_root_ptr().unwrap().ptr_eq(&ancestor));
            must!(call_object(
                cx,
                capability.resolve(cx),
                cx.undefined(),
                &[cx.undefined()]
            ));
        }
    }

    Ok(cx.undefined())
}

/// GatherAvailableAncestors (https://tc39.es/ecma262/#sec-gather-available-ancestors)
fn gather_available_ancestors(
    cx: Context,
    module: StackRoot<SourceTextModule>,
    gathered: &mut HashMap<ModuleId, StackRoot<SourceTextModule>>,
) {
    if let Some(async_parent_modules) = module.async_parent_modules_ptr() {
        for parent_module in async_parent_modules.as_slice() {
            if !gathered.contains_key(&parent_module.id())
                || parent_module
                    .cycle_root_ptr()
                    .unwrap()
                    .evaluation_error_ptr()
                    .is_none()
            {
                debug_assert!(parent_module.state() == ModuleState::EvaluatingAsync);
                debug_assert!(parent_module.evaluation_error_ptr().is_none());
                debug_assert!(parent_module.is_async_evaluation());
                debug_assert!(parent_module.pending_async_dependencies() > 0);

                let mut parent_module = parent_module.to_stack(cx);
                parent_module.dec_pending_async_dependencies();

                if parent_module.pending_async_dependencies() == 0 {
                    gathered.insert(parent_module.id(), parent_module);

                    if !parent_module.has_top_level_await() {
                        gather_available_ancestors(cx, parent_module, gathered);
                    }
                }

                continue;
            }
        }
    }
}

/// AsyncModuleExecutionRejected (https://tc39.es/ecma262/#sec-async-module-execution-rejected)
fn async_module_execution_rejected(
    cx: Context,
    mut module: StackRoot<SourceTextModule>,
    error: StackRoot<Value>,
) -> AllocResult<()> {
    if module.state() == ModuleState::Evaluated {
        debug_assert!(module.evaluation_error_ptr().is_some());
        return Ok(());
    }

    debug_assert!(module.state() == ModuleState::EvaluatingAsync);
    debug_assert!(module.is_async_evaluation());
    debug_assert!(module.evaluation_error_ptr().is_none());

    // Mark evaluation of module as complete with an error
    module.set_evaluation_error(*error);
    module.set_state(ModuleState::Evaluated);
    module.set_async_evaluation(cx, false);

    // Reject execution of all async parent modules as well
    if let Some(async_parent_modules) = module.async_parent_modules() {
        // Reuse handle between iterations
        let mut parent_module_handle: StackRoot<SourceTextModule> = StackRoot::empty(cx);

        for i in 0..async_parent_modules.len() {
            parent_module_handle.replace(async_parent_modules.as_slice()[i]);
            async_module_execution_rejected(cx, parent_module_handle, error)?;
        }
    }

    // If entire cycle has been completed, reject the top-level capability for the cycle
    if let Some(capability) = module.top_level_capability_ptr() {
        debug_assert!(module.cycle_root_ptr().unwrap().ptr_eq(&module));
        must_a!(call_object(
            cx,
            capability.reject(cx),
            cx.undefined(),
            &[error]
        ));
    }

    Ok(())
}

pub fn async_module_execution_rejected_runtime(
    mut cx: Context,
    _: StackRoot<Value>,
    arguments: &[StackRoot<Value>],
) -> EvalResult<StackRoot<Value>> {
    // Fetch the module passed from `execute_async_module`
    let current_function = cx.current_function();
    let module = get_module(cx, current_function);

    let error = get_argument(cx, arguments, 0);
    async_module_execution_rejected(cx, module, error)?;

    Ok(cx.undefined())
}

/// Start a dynamic import within a module, passing the argument provided to `import()`.
pub fn dynamic_import(
    cx: Context,
    source_file_path: StackRoot<FlatString>,
    specifier: StackRoot<Value>,
    options: StackRoot<Value>,
) -> EvalResult<StackRoot<ObjectValue>> {
    let promise_constructor = cx.get_intrinsic(Intrinsic::PromiseConstructor);
    let capability = must!(PromiseCapability::new(cx, promise_constructor.into()));

    let specifier_string_completion = to_string(cx, specifier);
    let specifier = if_abrupt_reject_promise!(cx, specifier_string_completion, capability);
    let sys = match cx.sys.as_ref() {
        Some(sys) => sys,
        None => {
            let error = type_error_value(cx, "Dynamic import not supported in this context")?;
            must!(call_object(
                cx,
                capability.reject(cx),
                cx.undefined(),
                &[error]
            ));
            return Ok(capability.promise(cx));
        }
    };
    let mut attribute_pairs = vec![];

    if !options.is_undefined() {
        if !options.is_object() {
            let error = type_error_value(cx, "Import options must be an object")?;
            must!(call_object(
                cx,
                capability.reject(cx),
                cx.undefined(),
                &[error]
            ));
            return Ok(capability.promise(cx));
        }

        let attributes_object_completion = get(cx, options.as_object(), cx.names.with());
        let attributes_object =
            if_abrupt_reject_promise!(cx, attributes_object_completion, capability);

        if !attributes_object.is_undefined() {
            if !attributes_object.is_object() {
                let error = type_error_value(cx, "Import attributes must be an object")?;
                must!(call_object(
                    cx,
                    capability.reject(cx),
                    cx.undefined(),
                    &[error]
                ));
                return Ok(capability.promise(cx));
            }

            let entries_completion = enumerable_own_property_names(
                cx,
                attributes_object.as_object(),
                KeyOrValue::KeyAndValue,
            );
            let entries = if_abrupt_reject_promise!(cx, entries_completion, capability);

            for entry in entries {
                // Entry is gaurenteed to be an array with two elements
                let entry = entry.as_object();
                let key = must!(get(cx, entry, PropertyKey::from_u8(0).to_stack(cx)));
                let value = must!(get(cx, entry, PropertyKey::from_u8(1).to_stack(cx)));

                if !value.is_string() {
                    let error = type_error_value(cx, "Import attribute values must be strings")?;
                    must!(call_object(
                        cx,
                        capability.reject(cx),
                        cx.undefined(),
                        &[error]
                    ));
                    return Ok(capability.promise(cx));
                }

                // Intern the key and value strings
                let key_string = must!(to_string(cx, key));
                let key_flat_string = key_string.flatten(cx)?;
                let key_interned_string = InternedStrings::get(cx, *key_flat_string)?.to_stack(cx);

                let value_flat_string = value.as_string().flatten(cx)?;
                let value_interned_string =
                    InternedStrings::get(cx, *value_flat_string)?.to_stack(cx);

                attribute_pairs.push((key_interned_string, value_interned_string));
            }
        }
    }

    let attributes = if !attribute_pairs.is_empty() {
        // Sort keys in lexicographic order
        attribute_pairs.sort_by_key(|(key, _)| *key);

        Some(ImportAttributes::new(cx, &attribute_pairs)?)
    } else {
        None
    };

    let specifier = specifier.flatten(cx)?;
    let specifier = InternedStrings::get(cx, *specifier)?.to_stack(cx);

    let module_request = ModuleRequest {
        specifier,
        attributes,
    };

    let load_completion = sys.host_load_imported_module(
        cx,
        &source_file_path.to_string(),
        module_request,
        cx.current_realm(),
    );
    continue_dynamic_import(cx, capability, load_completion)?;

    Ok(capability.promise(cx))
}

/// ContinueDynamicImport (https://tc39.es/ecma262/#sec-ContinueDynamicImport)
fn continue_dynamic_import(
    cx: Context,
    capability: StackRoot<PromiseCapability>,
    load_completion: EvalResult<DynModule>,
) -> AllocResult<()> {
    let module = match completion_value!(load_completion) {
        Ok(module) => module,
        Err(error) => {
            must_a!(call_object(
                cx,
                capability.reject(cx),
                cx.undefined(),
                &[error]
            ));
            return Ok(());
        }
    };

    let load_promise = module.load_requested_modules(cx)?;

    let on_resolve = callback(cx, load_requested_modules_dynamic_resolve)?;
    set_dyn_module(cx, on_resolve, module)?;
    set_capability(cx, on_resolve, capability)?;

    let on_reject = callback(cx, load_requested_modules_reject)?;
    set_capability(cx, on_reject, capability)?;

    perform_promise_then(cx, load_promise, on_resolve.into(), on_reject.into(), None)?;

    Ok(())
}

pub fn load_requested_modules_dynamic_resolve(
    mut cx: Context,
    _: StackRoot<Value>,
    _: &[StackRoot<Value>],
) -> EvalResult<StackRoot<Value>> {
    // Fetch the module and capability passed from the caller
    let current_function = cx.current_function();
    let module = get_dyn_module(cx, current_function);
    let capability = get_capability(cx, current_function);

    if let Err(error) = completion_value!(module.link(cx)) {
        must!(call_object(
            cx,
            capability.reject(cx),
            cx.undefined(),
            &[error]
        ));
        return Ok(cx.undefined());
    }

    // Missing condition in the spec. If the module has already been evaluated and throw an error
    // we should rethrow that error directly. Otherwise Evaluate will fail since it expects an
    // evaluated module to have a [[CycleRoot]], but [[CycleRoot]] is not set if module evaluation
    // errors.
    if let Some(module) = module.as_source_text_module() {
        if module.state() == ModuleState::Evaluated && module.evaluation_error_ptr().is_some() {
            must!(call_object(
                cx,
                capability.reject(cx),
                cx.undefined(),
                &[module.evaluation_error(cx).unwrap()]
            ));
            return Ok(cx.undefined());
        }
    }

    let evaluate_promise = module.evaluate(cx)?;

    let on_resolve = callback(cx, module_evaluate_dynamic_resolve)?;
    set_dyn_module(cx, on_resolve, module)?;
    set_capability(cx, on_resolve, capability)?;

    let on_reject = callback(cx, load_requested_modules_reject)?;
    set_capability(cx, on_reject, capability)?;

    perform_promise_then(
        cx,
        evaluate_promise,
        on_resolve.into(),
        on_reject.into(),
        None,
    )?;

    Ok(cx.undefined())
}

pub fn module_evaluate_dynamic_resolve(
    mut cx: Context,
    _: StackRoot<Value>,
    _: &[StackRoot<Value>],
) -> EvalResult<StackRoot<Value>> {
    // Fetch the module and capbility passed from the caller
    let current_function = cx.current_function();
    let mut module = get_dyn_module(cx, current_function);
    let capability = get_capability(cx, current_function);

    let namespace_object = module.get_namespace_object(cx)?.to_stack(cx);

    must!(call_object(
        cx,
        capability.resolve(cx),
        cx.undefined(),
        &[namespace_object.into()]
    ));

    Ok(cx.undefined())
}
