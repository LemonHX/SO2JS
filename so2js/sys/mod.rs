use alloc::{rc::Rc, string::ToString};

use crate::runtime::error::syntax_error;
use crate::{
    common::wtf_8::Wtf8String,
    parser::{analyze::analyze, parse_module, source::Source, ParseContext},
    runtime::{
        bytecode::generator::BytecodeProgramGenerator,
        context::ModuleCacheKey,
        error::syntax_parse_error,
        intrinsics::json_object::JSONObject,
        module::{module::DynModule, source_text_module::ModuleRequest},
        Context, EvalResult, Realm, StackRoot, Value,
    },
};
pub trait Sys {
    /// file/url canonicalization
    fn path_canonicalize(&self, path: &str) -> alloc::string::String;

    /// Get the current time in milliseconds since the UNIX epoch
    fn current_time_millis(&self) -> f64;

    /// HostLoadImportedModule (https://tc39.es/ecma262/#sec-HostLoadImportedModule)
    fn host_load_imported_module(
        &self,
        cx: Context,
        source_file_path: &str,
        module_request: ModuleRequest,
        realm: StackRoot<Realm>,
    ) -> EvalResult<DynModule>;

    fn host_load_imported_source_module(
        &self,
        mut cx: Context,
        realm: StackRoot<Realm>,
        module_request: ModuleRequest,
        new_module_path_string: &str,
        source_code: &str,
    ) -> EvalResult<DynModule> {
        let source = match Source::new_for_string(
            new_module_path_string,
            Wtf8String::from_str(&source_code),
        ) {
            Ok(source) => Rc::new(source),
            Err(error) => return syntax_parse_error(cx, &error),
        };

        // Parse the source, returning AST
        let pcx = ParseContext::new(source);
        let parse_result = match parse_module(&pcx, cx.options.clone()) {
            Ok(parse_result) => parse_result,
            Err(error) => return syntax_parse_error(cx, &error),
        };
        // Analyze AST
        let analyzed_result = match analyze(parse_result) {
            Ok(analyzed_result) => analyzed_result,
            Err(parse_errors) => return syntax_parse_error(cx, &parse_errors.errors[0]),
        };
        // Finally generate the SourceTextModule for the parsed module
        let bytecode_result = BytecodeProgramGenerator::generate_from_parse_module_result(
            cx,
            &Rc::new(analyzed_result),
            realm,
        );
        let module = match bytecode_result {
            Ok(module) => module,
            Err(error) => return syntax_error(cx, &error.to_string()),
        };
        // Cache the module
        let module_cache_key = ModuleCacheKey::new(
            new_module_path_string.to_string(),
            module_request.attributes,
        );
        cx.insert_module(module_cache_key, module.as_dyn_module())?;

        Ok(module.as_dyn_module())
    }

    fn parse_json_file_from_string(
        &self,
        mut cx: Context,
        string: &str,
    ) -> EvalResult<StackRoot<Value>> {
        JSONObject::parse(cx, cx.undefined(), &[cx.alloc_string(&string)?.as_value()])
    }
}
