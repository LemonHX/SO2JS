use alloc::string::ToString;
use hashbrown::HashSet;

use crate::runtime::alloc_error::AllocError;
use crate::{
    completion_value, must_a,
    runtime::{
        abstract_operations::call_object,
        alloc_error::AllocResult,
        eval_result::EvalResult,
        intrinsics::intrinsics::Intrinsic,
        promise_object::{PromiseCapability, PromiseObject},
        Context, Handle, Realm,
    },
};

use super::{
    module::{DynModule, ModuleId},
    source_text_module::{ModuleRequest, ModuleState, SourceTextModule},
};

/// GraphLoadingStateRecord (https://tc39.es/ecma262/#graphloadingstate-record)
struct GraphLoader {
    is_loading: bool,
    pending_modules_count: usize,
    visited: HashSet<ModuleId>,
    promise_capability: Handle<PromiseCapability>,
    realm: Handle<Realm>,
}

impl GraphLoader {
    /// InnerModuleLoading (https://tc39.es/ecma262/#sec-InnerModuleLoading)
    fn inner_module_loading(&mut self, cx: Context, module: DynModule) -> AllocResult<()> {
        let sys = cx.sys.as_ref().ok_or_else(|| AllocError::Oom(()))?;

        if let Some(mut module) = module.as_source_text_module() {
            if module.state() == ModuleState::New && self.visited.insert(module.id()) {
                module.set_state(ModuleState::Unlinked);

                let module_requests = module.requested_modules();
                let loaded_modules = module.loaded_modules();

                self.pending_modules_count += module_requests.len();

                for i in 0..module_requests.len() {
                    match loaded_modules.as_slice()[i] {
                        Some(loaded_module) => {
                            self.inner_module_loading(cx, DynModule::from_heap(&loaded_module))?
                        }
                        None => {
                            let module_request =
                                ModuleRequest::from_heap(&module_requests.as_slice()[i]);

                            // Create the SourceTextModule for the module with the given specifier,
                            // or evaluate to an error.
                            let load_result = sys.host_load_imported_module(
                                cx,
                                &module.source_file_path().to_string(),
                                module_request,
                                self.realm,
                            );

                            // Continue module loading with the SourceTextModule or error result
                            self.finish_loading_imported_module(
                                cx,
                                module,
                                module_request,
                                load_result,
                            )?;
                        }
                    }

                    if !self.is_loading {
                        return Ok(());
                    }
                }
            }
        }

        self.pending_modules_count -= 1;

        if self.pending_modules_count == 0 {
            self.is_loading = false;

            must_a!(call_object(
                cx,
                self.promise_capability.resolve(),
                cx.undefined(),
                &[cx.undefined()]
            ));
        }

        Ok(())
    }

    /// FinishLoadingImportedModule (https://tc39.es/ecma262/#sec-FinishLoadingImportedModule)
    fn finish_loading_imported_module(
        &mut self,
        cx: Context,
        mut referrer: Handle<SourceTextModule>,
        module_request: ModuleRequest,
        module_result: EvalResult<DynModule>,
    ) -> AllocResult<()> {
        if let Ok(module) = module_result {
            let module_index = referrer
                .lookup_module_request_index(&module_request.to_heap())
                .unwrap();
            if !referrer.has_loaded_module_at(module_index) {
                referrer.set_loaded_module_at(module_index, module);
            }
        }

        self.continue_module_loading(cx, module_result)
    }

    /// ContinueModuleLoading (https://tc39.es/ecma262/#sec-ContinueModuleLoading)
    fn continue_module_loading(
        &mut self,
        cx: Context,
        module_result: EvalResult<DynModule>,
    ) -> AllocResult<()> {
        if !self.is_loading {
            return Ok(());
        }

        match completion_value!(module_result) {
            Ok(module) => {
                self.inner_module_loading(cx, module)?;
            }
            Err(error) => {
                self.is_loading = false;
                must_a!(call_object(
                    cx,
                    self.promise_capability.reject(),
                    cx.undefined(),
                    &[error]
                ));
            }
        }

        Ok(())
    }
}

/// LoadRequestedModules (https://tc39.es/ecma262/#sec-LoadRequestedModules)
pub fn load_requested_modules(
    cx: Context,
    module: Handle<SourceTextModule>,
) -> AllocResult<Handle<PromiseObject>> {
    let promise_constructor = cx.get_intrinsic(Intrinsic::PromiseConstructor);
    let capability = must_a!(PromiseCapability::new(cx, promise_constructor.into()));
    let realm = module.program_function_ptr().realm();

    let mut graph_loader = GraphLoader {
        is_loading: true,
        pending_modules_count: 1,
        visited: HashSet::new(),
        promise_capability: capability,
        realm,
    };

    graph_loader.inner_module_loading(cx, module.as_dyn_module())?;

    // Known to be a PromiseObject since it was created by the intrinsic Promise constructor
    Ok(capability.promise().cast::<PromiseObject>())
}
