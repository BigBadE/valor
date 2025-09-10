//! Minimal V8-based JavaScript engine crate for Valor.
//!
//! This crate provides a narrow, swappable interface to a JavaScript engine with
//! a V8-backed implementation that is always enabled in this build.

use anyhow::{anyhow, Result};
use js::Console;
use js::{HostBindings, HostContext, HostFnKind, JSValue};
use js::JsEngine; // Use the engine-agnostic trait from js crate
use rusty_v8 as v8;
use std::convert::TryFrom;


/// Dispatcher for host-bound functions installed through HostBindings.
fn host_fn_dispatch(scope: &mut v8::HandleScope, args: v8::FunctionCallbackArguments, mut rv: v8::ReturnValue) {
    let data = match args.data() { Some(d) => d, None => { rv.set(v8::undefined(scope).into()); return; } };
    let ext = match v8::Local::<v8::External>::try_from(data) { Ok(e) => e, Err(_) => { rv.set(v8::undefined(scope).into()); return; } };
    let ptr = ext.value();
    if ptr.is_null() { rv.set(v8::undefined(scope).into()); return; }
    // SAFETY: pointer refers to a Box<(HostContext, HostFnKind)> leaked in make_v8_callback
    let payload: &(HostContext, HostFnKind) = unsafe { &*(ptr as *const (HostContext, HostFnKind)) };
    let (host_context, host_fn_kind) = payload;
    // Collect arguments
    let mut collected: Vec<JSValue> = Vec::new();
    for i in 0..args.length() {
        let value = args.get(i);
        if value.is_undefined() { collected.push(JSValue::Undefined); continue; }
        if value.is_null() { collected.push(JSValue::Null); continue; }
        if value.is_boolean() { collected.push(JSValue::Boolean(value.boolean_value(scope))); continue; }
        if value.is_number() { collected.push(JSValue::Number(value.number_value(scope).unwrap_or(f64::NAN))); continue; }
        if value.is_string() { collected.push(JSValue::String(value.to_string(scope).map(|s| s.to_rust_string_lossy(scope)).unwrap_or_default())); continue; }
        // Fallback: stringify non-primitive values via JS toString for console.*
        let stringified = value
            .to_string(scope)
            .map(|s| s.to_rust_string_lossy(scope))
            .unwrap_or_else(|| String::from("undefined"));
        collected.push(JSValue::String(stringified));
    }
    match host_fn_kind {
        HostFnKind::Sync(function) => match function(host_context, collected) {
            Ok(result) => {
                let v: v8::Local<v8::Value> = match result {
                    JSValue::Undefined => v8::undefined(scope).into(),
                    JSValue::Null => v8::null(scope).into(),
                    JSValue::Boolean(b) => v8::Boolean::new(scope, b).into(),
                    JSValue::Number(n) => v8::Number::new(scope, n).into(),
                    JSValue::String(s) => v8::String::new(scope, &s).unwrap().into(),
                };
                rv.set(v);
            }
            Err(error) => {
                let message = format!("{}", error);
                if let Some(js_message) = v8::String::new(scope, &message) {
                    scope.throw_exception(js_message.into());
                } else {
                    rv.set(v8::undefined(scope).into());
                }
            }
        },
    }
}

/// Holds the owned V8 isolate and the shared platform reference for the engine lifecycle.
struct OwnedIsolateWithHandleScope {
    isolate: v8::OwnedIsolate,
    _platform: v8::SharedRef<v8::Platform>,
}


/// V8-backed engine, always compiled.
pub struct V8Engine {
    inner: Option<v8::Global<v8::Context>>,
    isolate: Option<OwnedIsolateWithHandleScope>,
    stubs_installed: bool,
}

impl Default for V8Engine {
    fn default() -> Self {
        Self {
            inner: None,
            isolate: None,
            stubs_installed: false,
        }
    }
}

impl V8Engine {
    /// Create a new engine instance: initializes the V8 platform, isolate, and context.
    pub fn new() -> Result<Self> {
        // Initialize V8 platform (singleton acceptable per-process).
        static START: std::sync::Once = std::sync::Once::new();
        START.call_once(|| {
            let platform = v8::new_default_platform(0, false).make_shared();
            v8::V8::initialize_platform(platform);
            v8::V8::initialize();
        });

        let platform = v8::new_default_platform(0, false).make_shared();
        let isolate = v8::Isolate::new(v8::CreateParams::default());
        let mut owned = OwnedIsolateWithHandleScope {
            isolate,
            _platform: platform,
        };
        let global_context = {
            let scope = &mut v8::HandleScope::new(&mut owned.isolate);
            let context: v8::Local<v8::Context> = v8::Context::new(scope);
            v8::Global::new(scope, context)
        };
        Console::info("V8Engine initialized");
        Ok(Self {
            inner: Some(global_context),
            isolate: Some(owned),
            stubs_installed: false,
        })
    }

    /// Compile and run a script string within the current context.
    fn run_script_internal(&mut self, source: &str, url: &str) -> Result<()> {
        let isolate_container = self
            .isolate
            .as_mut()
            .ok_or_else(|| anyhow!("isolate not initialized"))?;
        let isolate = &mut isolate_container.isolate;
        let hs = &mut v8::HandleScope::new(isolate);
        let global_context = self
            .inner
            .as_ref()
            .ok_or_else(|| anyhow!("context not initialized"))?;
        let local_context: v8::Local<v8::Context> = v8::Local::new(hs, global_context);
        let cs = &mut v8::ContextScope::new(hs, local_context);
        let tc = &mut v8::TryCatch::new(cs);

        let code = v8::String::new(tc, source).ok_or_else(|| anyhow!("alloc v8 string"))?;
        let name = v8::String::new(tc, url).ok_or_else(|| anyhow!("alloc v8 name"))?;
        let undefined: v8::Local<v8::Value> = v8::undefined(tc).into();
        let origin = v8::ScriptOrigin::new(
            tc,
            name.into(),
            0,
            0,
            false,
            0,
            undefined,
            false,
            false,
            false,
        );
        if v8::Script::compile(tc, code, Some(&origin))
            .and_then(|script| script.run(tc))
            .is_none()
        {
            if tc.has_caught() {
                let exc = tc.exception();
                let exc_str = exc
                    .and_then(|e| e.to_string(tc))
                    .map(|s| s.to_rust_string_lossy(tc))
                    .unwrap_or_else(|| String::from("Uncaught exception"));
                let stack = tc
                    .stack_trace()
                    .and_then(|v| v.to_string(tc))
                    .map(|s| s.to_rust_string_lossy(tc));
                let message = if let Some(m) = tc.message() {
                    m.get(tc).to_rust_string_lossy(tc)
                } else {
                    exc_str.clone()
                };
                Console::exception(message, stack.as_deref());
            }
            return Err(anyhow!("v8 failed"));
        }
        Ok(())
    }


    /// Ensure minimal globals and install native console callbacks via the bindings facade.
    fn ensure_stubs(&mut self) -> Result<()> {
        if self.stubs_installed {
            return Ok(());
        }
        // Evaluate the engine-agnostic runtime prelude provided by the js crate.
        let prelude = js::runtime::runtime_prelude();
        let _ = self.run_script_internal(prelude, "valor://runtime_prelude");
        self.stubs_installed = true;
        Ok(())
    }
}

impl V8Engine {
    /// Convert a generic `JSValue` to a V8 `Local<Value>`.
    fn from_js_value<'s>(scope: &mut v8::HandleScope<'s>, value: &JSValue) -> v8::Local<'s, v8::Value> {
        match value {
            JSValue::Undefined => v8::undefined(scope).into(),
            JSValue::Null => v8::null(scope).into(),
            JSValue::Boolean(b) => v8::Boolean::new(scope, *b).into(),
            JSValue::Number(n) => v8::Number::new(scope, *n).into(),
            JSValue::String(s) => v8::String::new(scope, s).unwrap().into(),
        }
    }


    /// Wrap a `HostFnKind` as a V8 `Function`.
    fn make_v8_callback<'s>(
        scope: &mut v8::HandleScope<'s>,
        host_context: HostContext,
        host_fn: HostFnKind,
    ) -> v8::Local<'s, v8::Function> {
        // Allocate payload and leak it; V8 has no finalizer hook here. In practice this lives as long as the function.
        let payload = Box::new((host_context, host_fn));
        let ptr = Box::into_raw(payload) as *mut std::ffi::c_void;
        let external = v8::External::new(scope, ptr);
        v8::Function::builder(host_fn_dispatch).data(external.into()).build(scope).unwrap()
    }

    /// Install host bindings (namespaces and functions) onto the global object.
    pub fn install_bindings(&mut self, host_context: HostContext, bindings: &HostBindings) -> Result<()> {
        let isolate_container = self
            .isolate
            .as_mut()
            .ok_or_else(|| anyhow!("isolate not initialized"))?;
        let isolate = &mut isolate_container.isolate;
        let handle_scope = &mut v8::HandleScope::new(isolate);
        let global_context = self
            .inner
            .as_ref()
            .ok_or_else(|| anyhow!("context not initialized"))?;
        let local_context: v8::Local<v8::Context> = v8::Local::new(handle_scope, global_context);
        let scope = &mut v8::ContextScope::new(handle_scope, local_context);
        let global = local_context.global(scope);

        for (namespace_name, namespace) in &bindings.namespaces {
            // Merge into existing global object if present (e.g., document), otherwise create a new one.
            let ns_key = v8::String::new(scope, namespace_name).unwrap();
            let existing = global.get(scope, ns_key.into());
            let target_obj: v8::Local<v8::Object> = if let Some(val) = existing.and_then(|v| v8::Local::<v8::Object>::try_from(v).ok()) {
                val
            } else {
                let obj = v8::Object::new(scope);
                let _ = global.set(scope, ns_key.into(), obj.into());
                obj
            };

            // Install properties
            for (property_name, property_value) in &namespace.properties {
                let key = v8::String::new(scope, property_name).unwrap();
                let value = Self::from_js_value(scope, property_value);
                let _ = target_obj.set(scope, key.into(), value);
            }

            // Install functions
            for (function_name, function_kind) in &namespace.functions {
                let function = Self::make_v8_callback(scope, host_context.clone(), function_kind.clone());
                let key = v8::String::new(scope, function_name).unwrap();
                let _ = target_obj.set(scope, key.into(), function.into());
                // Expose host getElementById for the runtime wrapper to call directly, avoiding stale closures.
                if *namespace_name == "document" && *function_name == "getElementById" {
                    let host_key = v8::String::new(scope, "__valorHost_getElementById").unwrap();
                    let _ = target_obj.set(scope, host_key.into(), function.into());
                }
            }
        }
        Ok(())
    }
}

impl JsEngine for V8Engine {
    fn eval_script(&mut self, source: &str, url: &str) -> Result<()> {
        // Ensure stubs exist so basic console/document calls don't throw
        let _ = self.ensure_stubs();
        self.run_script_internal(source, url)
    }

    fn run_jobs(&mut self) -> Result<()> {
        // V8 runs microtasks at checkpoints; perform within a context and catch exceptions.
        if let Some(isolate_container) = &mut self.isolate {
            let isolate = &mut isolate_container.isolate;
            let hs = &mut v8::HandleScope::new(isolate);
            let global_context = self
                .inner
                .as_ref()
                .ok_or_else(|| anyhow!("context not initialized"))?;
            let local_context: v8::Local<v8::Context> = v8::Local::new(hs, global_context);
            let cs = &mut v8::ContextScope::new(hs, local_context);
            let tc = &mut v8::TryCatch::new(cs);
            tc.perform_microtask_checkpoint();
            if tc.has_caught() {
                let exc = tc.exception();
                let exc_str = exc
                    .and_then(|e| e.to_string(tc))
                    .map(|s| s.to_rust_string_lossy(tc))
                    .unwrap_or_else(|| String::from("Uncaught exception in microtask"));
                let stack = tc
                    .stack_trace()
                    .and_then(|v| v.to_string(tc))
                    .map(|s| s.to_rust_string_lossy(tc));
                let message = if let Some(m) = tc.message() {
                    m.get(tc).to_rust_string_lossy(tc)
                } else {
                    exc_str.clone()
                };
                Console::exception(message, stack.as_deref());
                return Err(anyhow!("v8 microtask exception"));
            }
        }
        Ok(())
    }
}


