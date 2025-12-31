use crate::{
    common::error::{ErrorFormatter, SourceInfo},
    runtime::alloc_error::AllocResult,
};
use alloc::string::String;
use alloc::string::ToString;
use alloc::{borrow::ToOwned, format};

use super::{
    heap_item_descriptor::HeapItemKind,
    intrinsics::{
        error_constructor::{CachedStackTraceInfo, ErrorObject},
        error_prototype::{error_message, error_name},
    },
    type_utilities::number_to_string,
    value::{BOOL_TAG, NULL_TAG, UNDEFINED_TAG},
    Context, StackRoot, Value,
};

/// Format for printing value to console
pub fn to_console_string(cx: Context, value: StackRoot<Value>) -> AllocResult<String> {
    let result = if value.is_pointer() {
        match value.as_pointer().descriptor().kind() {
            HeapItemKind::String => value.as_string().format()?,
            HeapItemKind::Symbol => match value.as_symbol().description_ptr() {
                None => String::from("Symbol()"),
                Some(description) => format!("Symbol({description})"),
            },
            HeapItemKind::BigInt => format!("{}n", value.as_bigint().bigint()),
            // Otherwise must be an object
            _ => {
                let object = value.as_object();

                if let Some(error) = object.as_error() {
                    error_to_console_string(cx, error)?
                } else if object.is_callable() {
                    "[Function]".to_owned()
                } else {
                    "[Object]".to_owned()
                }
            }
        }
    } else {
        match value.get_tag() {
            NULL_TAG => "null".to_owned(),
            UNDEFINED_TAG => "undefined".to_owned(),
            BOOL_TAG => {
                if value.as_bool() {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                }
            }
            // Otherwise must be a number, either a double or smi
            _ => number_to_string(value.as_number()),
        }
    };

    Ok(result)
}

fn error_to_console_string(cx: Context, mut error: StackRoot<ErrorObject>) -> AllocResult<String> {
    let name = error_name(cx, error).format()?;
    let mut formatter = ErrorFormatter::new(name);

    if let Some(message) = error_message(cx, error)? {
        formatter.set_message(message);
    }

    let stack_trace = error.get_stack_trace(cx)?;
    formatter.set_stack_trace(stack_trace.frames.to_string());

    if let Some(source_info) = new_heap_source_info(cx, &stack_trace)? {
        formatter.set_source_info(source_info);
    }

    Ok(formatter.build())
}

fn new_heap_source_info(
    cx: Context,
    stack_trace_info: &CachedStackTraceInfo,
) -> AllocResult<Option<SourceInfo>> {
    let (mut source_file, line, col) =
        if let Some((source_file, line, col)) = &stack_trace_info.source_file_line_col {
            (source_file.to_stack(), *line, *col)
        } else {
            return Ok(None);
        };

    let name = source_file.display_name().to_string();
    let snippet = source_file.get_line(cx, line - 1)?;

    Ok(Some(SourceInfo::new(name, line, col, snippet)))
}
