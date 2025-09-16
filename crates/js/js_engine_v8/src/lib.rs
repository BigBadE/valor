//! Minimal V8-based JavaScript engine crate for Valor.
//!
//! This crate provides a narrow, swappable interface to a JavaScript engine with
//! a V8-backed implementation that is always enabled in this build.

use anyhow::{Result, anyhow};
use core::convert::TryFrom as _;
use core::ffi::c_void;
use js::Console;
use js::JsEngine; // Use the engine-agnostic trait from js crate
use js::runtime::runtime_prelude;
use js::{HostBindings, HostContext, HostFnKind, JSValue};
use rusty_v8::{
    Boolean, Context, ContextScope, CreateParams, External, Function, FunctionCallbackArguments,
    Global, HandleScope, Isolate, Local, Module, Number, Object, OwnedIsolate, Platform,
    ReturnValue, Script, ScriptOrigin, SharedRef, String as V8String, TryCatch, V8, Value,
    new_default_platform, null, undefined,
};
use std::collections::HashMap;
use std::sync::Once;

/// Escape a string so it can safely be embedded as a JavaScript literal.
#[inline]
fn escape_js_for_literal(input: &str) -> String {
    let mut out = String::with_capacity(input.len().saturating_add(8));
    for character in input.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(character),
        }
    }
    out
}

/// Produce a small script that calls `window.onerror(message, url, 0, 0)` if available.
#[inline]
fn build_onerror_script(message: &str, url: &str) -> String {
    let msg_lit = format!("\"{}\"", escape_js_for_literal(message));
    let url_lit = format!("\"{}\"", escape_js_for_literal(url));
    format!(
        "(function(m,u){{try{{if(typeof window!=='undefined'&&typeof window.onerror==='function'){{window.onerror(m,u,0,0);}}}}catch(_o){{}}}})({msg_lit},{url_lit});"
    )
}

/// Dispatcher for host-bound functions installed through `HostBindings`.
#[allow(clippy::needless_pass_by_value, reason = "Required for v8 callback")]
fn host_fn_dispatch(
    scope: &mut HandleScope,
    args: FunctionCallbackArguments,
    mut ret_val: ReturnValue,
) {
    // Read the external payload pointer first
    let Some(data_value) = args.data() else {
        ret_val.set(undefined(scope).into());
        return;
    };
    let Ok(external_value) = Local::<External>::try_from(data_value) else {
        ret_val.set(undefined(scope).into());
        return;
    };
    // Now consume the V8 arguments into JSValue list to satisfy pedantic and avoid borrowing issues
    let collected = collect_js_args(scope, &args);
    let ptr = external_value.value();
    if ptr.is_null() {
        ret_val.set(undefined(scope).into());
        return;
    }
    // SAFETY: pointer refers to a Box<(HostContext, HostFnKind)> leaked in make_v8_callback
    let payload: &(HostContext, HostFnKind) = unsafe { &*ptr.cast::<(HostContext, HostFnKind)>() };
    let host_context: &HostContext = &payload.0;
    let host_fn_kind: &HostFnKind = &payload.1;

    #[allow(
        clippy::pattern_type_mismatch,
        reason = "Not sure how to fix this pattern type mismatch"
    )]
    let HostFnKind::Sync(function_arc) = host_fn_kind;
    match (**function_arc)(host_context, collected) {
        Ok(result) => ret_val.set(jsvalue_to_local(scope, result)),
        Err(error) => {
            let message = format!("{error}");
            if let Some(js_message) = V8String::new(scope, &message) {
                scope.throw_exception(js_message.into());
            } else {
                ret_val.set(undefined(scope).into());
            }
        }
    }
}

/// Collect V8 callback arguments into engine-agnostic `JSValue`s.
#[inline]
fn collect_js_args(scope: &mut HandleScope, args: &FunctionCallbackArguments) -> Vec<JSValue> {
    let mut collected: Vec<JSValue> = Vec::new();
    let length = args.length();
    let start: i32 = 0;
    for index in start..length {
        let value = args.get(index);
        if value.is_undefined() {
            collected.push(JSValue::Undefined);
            continue;
        }
        if value.is_null() {
            collected.push(JSValue::Null);
            continue;
        }
        if value.is_boolean() {
            collected.push(JSValue::Boolean(value.boolean_value(scope)));
            continue;
        }
        if value.is_number() {
            collected.push(JSValue::Number(
                value.number_value(scope).unwrap_or(f64::NAN),
            ));
            continue;
        }
        if value.is_string() {
            collected.push(JSValue::String(
                value
                    .to_string(scope)
                    .map_or_else(String::new, |val_str| val_str.to_rust_string_lossy(scope)),
            ));
            continue;
        }
        let stringified = value.to_string(scope).map_or_else(
            || String::from("undefined"),
            |val_str| val_str.to_rust_string_lossy(scope),
        );
        collected.push(JSValue::String(stringified));
    }
    collected
}

#[inline]
/// Convert a `JSValue` into a V8 `Local<Value>`.
///
/// # Arguments
///
/// * `scope`: The V8 handle scope.
/// * `value`: The `JSValue` to convert.
///
/// # Returns
/// The converted `Local<Value>`.
fn jsvalue_to_local<'scope>(
    scope: &mut HandleScope<'scope>,
    value: JSValue,
) -> Local<'scope, Value> {
    match value {
        JSValue::Undefined => undefined(scope).into(),
        JSValue::Null => null(scope).into(),
        JSValue::Boolean(boolean_value) => Boolean::new(scope, boolean_value).into(),
        JSValue::Number(number_value) => Number::new(scope, number_value).into(),
        JSValue::String(string_value) => V8String::new(scope, string_value.as_str())
            .map_or_else(|| undefined(scope).into(), Into::into),
    }
}

/// Holds the owned V8 isolate and the shared platform reference for the engine lifecycle.
struct OwnedIsolateWithHandleScope {
    /// The owned V8 isolate for this engine instance.
    isolate: OwnedIsolate,
    /// The platform reference kept alive for the lifetime of the isolate.
    _platform: SharedRef<Platform>,
}

/// V8-backed engine, always compiled.
#[derive(Default)]
pub struct V8Engine {
    /// The current global V8 context.
    inner: Option<Global<Context>>,
    /// The owned isolate holder used to manage V8 lifetimes.
    isolate: Option<OwnedIsolateWithHandleScope>,
    /// Whether the runtime prelude has been installed in the context.
    stubs_installed: bool,
    /// Optional base URL used for resolving relative script/module paths.
    _base_url: Option<String>,
    /// Registry of compiled ES modules keyed by absolute URL/specifier.
    _module_map: HashMap<String, Global<Module>>,
}

impl V8Engine {
    /// Create a new engine instance: initializes the V8 platform, isolate, and context.
    ///
    /// # Errors
    /// Returns an error if context initialization fails (unexpected).
    #[inline]
    pub fn new() -> Result<Self> {
        // Initialize V8 platform (singleton acceptable per-process).
        static START: Once = Once::new();
        START.call_once(|| {
            let platform = new_default_platform(0, false).make_shared();
            V8::initialize_platform(platform);
            V8::initialize();
        });

        let platform = new_default_platform(0, false).make_shared();
        let isolate = Isolate::new(CreateParams::default());
        let mut owned = OwnedIsolateWithHandleScope {
            isolate,
            _platform: platform,
        };
        let global_context = {
            let scope = &mut HandleScope::new(&mut owned.isolate);
            let context: Local<Context> = Context::new(scope);
            Global::new(scope, context)
        };
        Console::info("V8Engine initialized");
        Ok(Self {
            inner: Some(global_context),
            isolate: Some(owned),
            stubs_installed: false,
            _base_url: None,
            _module_map: HashMap::new(),
        })
    }

    /// Compile and run a script string within the current context.
    ///
    /// # Arguments
    ///
    /// * `source`: The script source code.
    /// * `url`: The script URL.
    ///
    /// # Errors
    /// Returns an error if compilation or execution fails.
    fn run_script_internal(&mut self, source: &str, url: &str) -> Result<()> {
        let isolate_container = self
            .isolate
            .as_mut()
            .ok_or_else(|| anyhow!("isolate not initialized"))?;
        let isolate = &mut isolate_container.isolate;
        let handle_scope = &mut HandleScope::new(isolate);
        let global_context = self
            .inner
            .as_ref()
            .ok_or_else(|| anyhow!("context not initialized"))?;
        let local_context: Local<Context> = Local::new(handle_scope, global_context);
        let context_scope = &mut ContextScope::new(handle_scope, local_context);
        let try_catch = &mut TryCatch::new(context_scope);

        let code = V8String::new(try_catch, source).ok_or_else(|| anyhow!("alloc v8 string"))?;
        let name = V8String::new(try_catch, url).ok_or_else(|| anyhow!("alloc v8 name"))?;
        let undef1: Local<Value> = undefined(try_catch).into();
        let origin = ScriptOrigin::new(
            try_catch,
            name.into(),
            0,
            0,
            false,
            0,
            undef1,
            false,
            false,
            false,
        );
        let failed = Script::compile(try_catch, code, Some(&origin))
            .is_some_and(|compiled| compiled.run(try_catch).is_none());
        if failed {
            if !try_catch.has_caught() {
                return Err(anyhow!("v8 failed"));
            }
            let exc = try_catch.exception();
            let exc_str = exc.and_then(|val| val.to_string(try_catch)).map_or_else(
                || "Uncaught exception".to_owned(),
                |val| val.to_rust_string_lossy(try_catch),
            );
            let stack = try_catch
                .stack_trace()
                .and_then(|val| val.to_string(try_catch))
                .map(|val| val.to_rust_string_lossy(try_catch));
            let message = try_catch.message().map_or_else(
                || exc_str.clone(),
                |msg_obj| msg_obj.get(try_catch).to_rust_string_lossy(try_catch),
            );
            Console::exception(message.clone(), stack.as_deref());
            let call_onerror = build_onerror_script(&message, url);
            if let Some(code2) = V8String::new(try_catch, &call_onerror) {
                let undef2: Local<Value> = undefined(try_catch).into();
                let origin2 = ScriptOrigin::new(
                    try_catch,
                    name.into(),
                    0,
                    0,
                    false,
                    0,
                    undef2,
                    false,
                    false,
                    false,
                );
                if let Some(compiled2) = Script::compile(try_catch, code2, Some(&origin2))
                    && compiled2.run(try_catch).is_none()
                {
                    Console::info("window.onerror dispatch failed");
                }
            }
            return Err(anyhow!("v8 failed"));
        }
        Ok(())
    }

    /// Ensure minimal globals and install native console callbacks via the bindings facade.
    ///
    /// # Errors
    /// Returns an error if executing the runtime prelude fails.
    fn ensure_stubs(&mut self) -> Result<()> {
        if self.stubs_installed {
            return Ok(());
        }
        // Evaluate the engine-agnostic runtime prelude provided by the js crate.
        let prelude = runtime_prelude();
        self.run_script_internal(prelude, "valor://runtime_prelude")?;
        self.stubs_installed = true;
        Ok(())
    }

    /// Convert a generic `JSValue` to a V8 `Local<Value>`.
    ///
    /// # Arguments
    ///
    /// * `scope`: The V8 handle scope.
    /// * `value`: The `JSValue` to convert.
    ///
    /// # Returns
    /// The converted `Local<Value>`.
    fn from_js_value<'scope>(
        scope: &mut HandleScope<'scope>,
        value: &JSValue,
    ) -> Local<'scope, Value> {
        // Avoid pattern matching on a reference to satisfy clippy pedantic.
        jsvalue_to_local(scope, value.clone())
    }

    /// Wrap a `HostFnKind` as a V8 `Function`.
    ///
    /// # Arguments
    ///
    /// * `scope`: The V8 handle scope.
    /// * `host_context`: The host context.
    /// * `host_fn`: The host function kind.
    ///
    /// # Returns
    /// The wrapped `Function` or `None` if allocation fails.
    fn make_v8_callback<'scope>(
        scope: &mut HandleScope<'scope>,
        host_context: &HostContext,
        host_fn: HostFnKind,
    ) -> Option<Local<'scope, Function>> {
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
    /// * `host_context`: The host context.
    /// * `bindings`: The host bindings.
    ///
    /// # Errors
    /// Returns an error if the V8 context or isolate are not initialized, or if
    /// V8 string allocation fails for any namespace/property/function identifiers.
    #[inline]
    pub fn install_bindings(
        &mut self,
        host_context: &HostContext,
        bindings: &HostBindings,
    ) -> Result<()> {
        let isolate_container = self
            .isolate
            .as_mut()
            .ok_or_else(|| anyhow!("isolate not initialized"))?;
        let isolate = &mut isolate_container.isolate;
        let handle_scope = &mut HandleScope::new(isolate);
        let global_context = self
            .inner
            .as_ref()
            .ok_or_else(|| anyhow!("context not initialized"))?;
        let local_context: Local<Context> = Local::new(handle_scope, global_context);
        let scope = &mut ContextScope::new(handle_scope, local_context);
        let global = local_context.global(scope);

        for (namespace_name, namespace) in &bindings.namespaces {
            // Merge into existing global object if present (e.g., document), otherwise create a new one.
            let ns_key = V8String::new(scope, namespace_name)
                .ok_or_else(|| anyhow!("failed to allocate V8 string for namespace"))?;
            let existing = global.get(scope, ns_key.into());
            let target_obj: Local<Object> = existing
                .and_then(|value| Local::<Object>::try_from(value).ok())
                .map_or_else(
                    || {
                        let obj = Object::new(scope);
                        let _set_ns: Option<bool> = global.set(scope, ns_key.into(), obj.into());
                        obj
                    },
                    |val| val,
                );

            // Install properties
            for (property_name, property_value) in &namespace.properties {
                let key = V8String::new(scope, property_name)
                    .ok_or_else(|| anyhow!("failed to allocate V8 string for property"))?;
                let value = Self::from_js_value(scope, property_value);
                let _set_prop: Option<bool> = target_obj.set(scope, key.into(), value);
            }

            // Install functions
            for (function_name, function_kind) in &namespace.functions {
                let maybe_function =
                    Self::make_v8_callback(scope, host_context, function_kind.clone());
                // Skip if function allocation failed to reduce nesting
                let Some(function) = maybe_function else {
                    continue;
                };
                let key = V8String::new(scope, function_name)
                    .ok_or_else(|| anyhow!("failed to allocate V8 string for function name"))?;
                let _set_fn: Option<bool> = target_obj.set(scope, key.into(), function.into());
                if *namespace_name != "document" {
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
        Ok(())
    }
}

impl JsEngine for V8Engine {
    #[inline]
    fn eval_script(&mut self, source: &str, url: &str) -> Result<()> {
        // Ensure stubs exist so basic console/document calls don't throw
        self.ensure_stubs()?;
        self.run_script_internal(source, url)
    }

    #[inline]
    fn eval_module(&mut self, source: &str, url: &str) -> Result<()> {
        // Minimal module support for Phase 6: evaluate pre-bundled side-effect code
        // using the classic script path. A future iteration can switch to true V8
        // module compilation and instantiation once the static import graph is wired.
        self.ensure_stubs()?;
        self.run_script_internal(source, url)
    }

    #[inline]
    fn run_jobs(&mut self) -> Result<()> {
        // V8 runs microtasks at checkpoints; perform within a context and catch exceptions.
        if let Some(isolate_container) = self.isolate.as_mut() {
            let isolate = &mut isolate_container.isolate;
            let handle_scope = &mut HandleScope::new(isolate);
            let global_context = self
                .inner
                .as_ref()
                .ok_or_else(|| anyhow!("context not initialized"))?;
            let local_context: Local<Context> = Local::new(handle_scope, global_context);
            let context_scope = &mut ContextScope::new(handle_scope, local_context);
            let try_catch = &mut TryCatch::new(context_scope);
            try_catch.perform_microtask_checkpoint();
            if try_catch.has_caught() {
                // Ignore microtask exceptions here; they were already surfaced during script execution.
            }
        }
        Ok(())
    }
}
