use super::{
    abstract_operations::define_property_or_throw, object_value::ObjectValue,
    property_descriptor::PropertyDescriptor, property_key::PropertyKey, string_value::StringValue,
    value::Value, Context, StackRoot,
};
use crate::{
    must, must_a,
    runtime::{alloc_error::AllocResult, EvalResult},
};
use alloc::format;
use core::error::Error;

/// SetFunctionName (https://tc39.es/ecma262/#sec-setfunctionname)
pub fn set_function_name(
    cx: Context,
    func: StackRoot<ObjectValue>,
    name: StackRoot<PropertyKey>,
    prefix: Option<&str>,
) -> AllocResult<()> {
    let name_string = build_function_name(cx, name, prefix)?;
    let desc = PropertyDescriptor::data(name_string.into(), false, false, true);
    must_a!(define_property_or_throw(cx, func, cx.names.name(), desc));

    Ok(())
}

pub fn build_function_name(
    mut cx: Context,
    name: StackRoot<PropertyKey>,
    prefix: Option<&str>,
) -> AllocResult<StackRoot<StringValue>> {
    // Convert name to string value, property formatting symbol name
    let name_string = if name.is_symbol() {
        let symbol = name.as_symbol();
        if let Some(description) = symbol.description() {
            if symbol.is_private() {
                StringValue::concat(
                    cx,
                    cx.alloc_string("#")?.as_string(),
                    description.as_string(),
                )?
            } else {
                let left_paren = cx.alloc_string("[")?.as_string();
                let right_paren = cx.alloc_string("]")?.as_string();

                StringValue::concat_all(cx, &[left_paren, description.as_string(), right_paren])?
            }
        } else {
            cx.names.empty_string().as_string()
        }
    } else {
        name.to_value(cx)?.as_string()
    };

    // Add prefix to name
    if let Some(prefix) = prefix {
        let prefix_string = cx.alloc_string(&format!("{prefix} "))?.as_string();
        StringValue::concat(cx, prefix_string, name_string)
    } else {
        Ok(name_string)
    }
}

/// SetFunctionLength (https://tc39.es/ecma262/#sec-setfunctionlength)
pub fn set_function_length(cx: Context, func: StackRoot<ObjectValue>, length: u32) -> AllocResult<()> {
    let length_value = Value::from(length).to_stack();
    let desc = PropertyDescriptor::data(length_value, false, false, true);
    must_a!(define_property_or_throw(cx, func, cx.names.length(), desc));

    Ok(())
}

// Identical to SetFunctionLength, but a None value represents a length of positive infinity
pub fn set_function_length_maybe_infinity(
    cx: Context,
    func: StackRoot<ObjectValue>,
    length: Option<usize>,
) -> EvalResult<()> {
    let length = if let Some(length) = length {
        Value::from(length).to_stack()
    } else {
        cx.number(f64::INFINITY)
    };

    let desc = PropertyDescriptor::data(length, false, false, true);
    must!(define_property_or_throw(cx, func, cx.names.length(), desc));

    Ok(())
}

pub fn get_argument(cx: Context, arguments: &[StackRoot<Value>], i: usize) -> StackRoot<Value> {
    if i < arguments.len() {
        arguments[i]
    } else {
        cx.undefined()
    }
}
