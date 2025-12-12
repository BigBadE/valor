//! Host function bindings and installation.

use crate::conversions::{collect_js_args, jsvalue_to_local};
use anyhow::{Result, anyhow};
use core::ffi::c_void;
use js::{HostBindings, HostContext, HostFnKind, HostNamespace, JSValue};
use v8::{
    Context, ContextScope, External, Function, FunctionCallbackArguments, Global, Isolate, Local,
    Object, ReturnValue, String as V8String, Value,
};

/// Dispatcher for host-bound functions installed through `HostBindings`.
pub(crate) fn host_fn_dispatch<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    args: FunctionCallbackArguments,
    mut ret_val: ReturnValue,
) {
    // Read the external payload pointer first
    let data_value = args.data();
    let Ok(external_value) = Local::<External>::try_from(data_value) else {
        let undef = v8::undefined(scope);
        ret_val.set(undef.into());
        return;
    };
    // Now consume the V8 arguments into JSValue list to satisfy pedantic and avoid borrowing issues
    let collected = collect_js_args(scope, &args);
    let ptr = external_value.value();
    if ptr.is_null() {
        let undef = v8::undefined(scope);
        ret_val.set(undef.into());
        return;
    }
    // SAFETY: pointer refers to a Box<(HostContext, HostFnKind)> leaked in make_v8_callback
    let payload: &(HostContext, HostFnKind) = unsafe { &*ptr.cast::<(HostContext, HostFnKind)>() };
    let host_context: &HostContext = &payload.0;
    let host_fn_kind: &HostFnKind = &payload.1;

    let HostFnKind::Sync(function_arc) = host_fn_kind;
    match (**function_arc)(host_context, collected) {
        Ok(result) => ret_val.set(jsvalue_to_local(scope, result)),
        Err(error) => {
            let message = format!("{error}");
            if let Some(js_message) = V8String::new(scope, &message) {
                let exc = v8::Exception::error(scope, js_message);
                scope.throw_exception(exc);
            } else {
                let undef = v8::undefined(scope);
                ret_val.set(undef.into());
            }
        }
    }
}

/// Wrap a `HostFnKind` as a V8 `Function`.
///
/// # Arguments
///
/// * `scope`: The V8 pin scope.
/// * `host_context`: The host context.
/// * `host_fn`: The host function kind.
///
/// # Returns
/// The wrapped `Function` or `None` if allocation fails.
pub(crate) fn make_v8_callback<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    host_context: &HostContext,
    host_fn: HostFnKind,
) -> Option<Local<'s, Function>> {
    // Allocate payload and leak it; V8 has no finalizer hook here. In practice this lives as long as the function.
    let payload = Box::new((host_context.clone(), host_fn));
    let ptr = Box::into_raw(payload).cast::<c_void>();
    let external = External::new(scope, ptr);
    Function::builder(host_fn_dispatch)
        .data(external.into())
        .build(scope)
}

/// Install host bindings (namespaces and functions) onto the global object.
///
/// # Arguments
///
/// * `isolate_mut`: Mutable isolate reference.
/// * `context`: Global context.
/// * `host_context`: The host context.
/// * `bindings`: The host bindings.
///
/// # Errors
/// Returns an error if the V8 context or isolate are not initialized, or if
/// V8 string allocation fails for any namespace/property/function identifiers.
pub(crate) fn install_bindings_impl(
    isolate_mut: &mut Isolate,
    context: &Global<Context>,
    host_context: &HostContext,
    bindings: &HostBindings,
) -> Result<()> {
    v8::scope!(let scope, isolate_mut);

    let local_context: Local<Context> = Local::new(scope, context);
    let scope = &mut ContextScope::new(scope, local_context);
    let global = local_context.global(scope);

    for (namespace_name, namespace) in &bindings.namespaces {
        // Merge into existing global object if present (e.g., document), otherwise create a new one.
        let ns_key = V8String::new(scope, namespace_name)
            .ok_or_else(|| anyhow!("failed to allocate V8 string for namespace"))?;
        let existing = global.get(scope, ns_key.into());
        let target_obj: Local<Object> = existing
            .and_then(|value| Local::<Object>::try_from(value).ok())
            .unwrap_or_else(|| {
                let obj = Object::new(scope);
                let _set_ns: Option<bool> = global.set(scope, ns_key.into(), obj.into());
                obj
            });

        install_namespace_properties(scope, namespace, target_obj);
        install_namespace_functions(scope, host_context, namespace_name, namespace, target_obj);
    }
    Ok(())
}

/// Install constant properties for a single host namespace onto the target object.
#[inline]
fn install_namespace_properties<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    namespace: &HostNamespace,
    target_obj: Local<Object>,
) {
    for (property_name, property_value) in &namespace.properties {
        let Some(key) = V8String::new(scope, property_name) else {
            continue;
        };
        let value = from_js_value(scope, property_value);
        let _set_prop: Option<bool> = target_obj.set(scope, key.into(), value);
    }
}

/// Install functions for a single host namespace onto the target object,
/// adding `__valorHost_*` aliases for the `document` namespace.
#[inline]
fn install_namespace_functions<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    host_context: &HostContext,
    namespace_name: &str,
    namespace: &HostNamespace,
    target_obj: Local<Object>,
) {
    for (function_name, function_kind) in &namespace.functions {
        let Some(function) = make_v8_callback(scope, host_context, function_kind.clone()) else {
            continue;
        };
        let Some(key) = V8String::new(scope, function_name) else {
            continue;
        };
        let _set_fn: Option<bool> = target_obj.set(scope, key.into(), function.into());
        if namespace_name != "document" {
            continue;
        }
        let host_alias = format!("__valorHost_{function_name}");
        let Some(alias_key) = V8String::new(scope, &host_alias) else {
            continue;
        };
        let _unused_alias_result: Option<bool> =
            target_obj.set(scope, alias_key.into(), function.into());
    }
}

/// Convert a generic `JSValue` to a V8 `Local<Value>`.
///
/// # Arguments
///
/// * `scope`: The V8 pin scope.
/// * `value`: The `JSValue` to convert.
///
/// # Returns
/// The converted `Local<Value>`.
fn from_js_value<'s, 'i>(scope: &mut v8::PinScope<'s, 'i>, value: &JSValue) -> Local<'s, Value> {
    // Avoid pattern matching on a reference to satisfy clippy pedantic.
    jsvalue_to_local(scope, value.clone())
}
