//! Minimal V8-based JavaScript engine crate for Valor.
//!
//! This crate provides a narrow, swappable interface to a JavaScript engine with
//! a V8-backed implementation that is always enabled in this build.

use anyhow::{Result, anyhow};
use js::Console;
use js::JsEngine; // Use the engine-agnostic trait from js crate
use js::{HostBindings, HostContext, HostFnKind, JSValue};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::ffi::c_void;
use std::sync::Once;
use rusty_v8::{new_default_platform, null, undefined, Boolean, Context, ContextScope, CreateParams, External, Function, FunctionCallbackArguments, Global, HandleScope, Isolate, Local, Module, Number, Object, OwnedIsolate, Platform, ReturnValue, Script, ScriptOrigin, SharedRef, String as V8String, TryCatch, Value, V8};

/// Dispatcher for host-bound functions installed through HostBindings.
fn host_fn_dispatch(
    scope: &mut HandleScope<()>,
    args: FunctionCallbackArguments,
    mut rv: ReturnValue,
) {
    let data = match args.data() {
        Some(d) => d,
        None => {
            rv.set(undefined(scope).into());
            return;
        }
    };
    let ext = match Local::<External>::try_from(data) {
        Ok(e) => e,
        Err(_) => {
            rv.set(undefined(scope).into());
            return;
        }
    };
    let ptr = ext.value();
    if ptr.is_null() {
        rv.set(undefined(scope).into());
        return;
    }
    // SAFETY: pointer refers to a Box<(HostContext, HostFnKind)> leaked in make_v8_callback
    let payload: &(HostContext, HostFnKind) =
        unsafe { &*(ptr as *const (HostContext, HostFnKind)) };
    let (host_context, host_fn_kind) = payload;
    // Collect arguments
    let mut collected: Vec<JSValue> = Vec::new();
    for i in 0..args.length() {
        let value = args.get(i);
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
                    .map(|s| s.to_rust_string_lossy(scope))
                    .unwrap_or_default(),
            ));
            continue;
        }
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
                let v: Local<Value> = match result {
                    JSValue::Undefined => undefined(scope).into(),
                    JSValue::Null => null(scope).into(),
                    JSValue::Boolean(b) => Boolean::new(scope, b).into(),
                    JSValue::Number(n) => Number::new(scope, n).into(),
                    JSValue::String(s) => V8String::new(scope, &s).unwrap().into(),
                };
                rv.set(v);
            }
            Err(error) => {
                let message = format!("{}", error);
                if let Some(js_message) = V8String::new(scope, &message) {
                    scope.throw_exception(js_message.into());
                } else {
                    rv.set(undefined(scope).into());
                }
            }
        },
    }
}

/// Holds the owned V8 isolate and the shared platform reference for the engine lifecycle.
struct OwnedIsolateWithHandleScope {
    isolate: OwnedIsolate,
    _platform: SharedRef<Platform>,
}

/// V8-backed engine, always compiled.
#[derive(Default)]
pub struct V8Engine {
    inner: Option<Global<Context>>,
    isolate: Option<OwnedIsolateWithHandleScope>,
    stubs_installed: bool,
    #[allow(dead_code)]
    base_url: Option<String>,
    /// Registry of compiled ES modules keyed by absolute URL/specifier.
    #[allow(dead_code)]
    module_map: HashMap<String, Global<Module>>,
}

impl V8Engine {
    /// Create a new engine instance: initializes the V8 platform, isolate, and context.
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
            base_url: None,
            module_map: HashMap::new(),
        })
    }

    /// Compile and run a script string within the current context.
    fn run_script_internal(&mut self, source: &str, url: &str) -> Result<()> {
        let isolate_container = self
            .isolate
            .as_mut()
            .ok_or_else(|| anyhow!("isolate not initialized"))?;
        let isolate = &mut isolate_container.isolate;
        let hs = &mut HandleScope::new(isolate);
        let global_context = self
            .inner
            .as_ref()
            .ok_or_else(|| anyhow!("context not initialized"))?;
        let local_context: Local<Context> = Local::new(hs, global_context);
        let cs = &mut ContextScope::new(hs, local_context);
        let tc = &mut TryCatch::new(cs);

        let code = V8String::new(tc, source).ok_or_else(|| anyhow!("alloc v8 string"))?;
        let name = V8String::new(tc, url).ok_or_else(|| anyhow!("alloc v8 name"))?;
        let undefined1: Local<Value> = undefined(tc).into();
        let origin = ScriptOrigin::new(
            tc,
            name.into(),
            0,
            0,
            false,
            0,
            undefined1,
            false,
            false,
            false,
        );
        if Script::compile(tc, code, Some(&origin))
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
                Console::exception(message.clone(), stack.as_deref());
                // Also propagate to window.onerror if present (Phase 6: error propagation)
                // Build a small JS snippet that calls window.onerror(message, url, 0, 0)
                fn escape_js(s: &str) -> String {
                    let mut out = String::with_capacity(s.len() + 8);
                    for ch in s.chars() {
                        match ch {
                            '\\' => out.push_str("\\\\"),
                            '"' => out.push_str("\\\""),
                            '\n' => out.push_str("\\n"),
                            '\r' => out.push_str("\\r"),
                            _ => out.push(ch),
                        }
                    }
                    out
                }
                let msg_lit = format!("\"{}\"", escape_js(&message));
                let url_lit = format!("\"{}\"", escape_js(url));
                let call_onerror = format!(
                    "(function(m,u){{try{{if(typeof window!=='undefined'&&typeof window.onerror==='function'){{window.onerror(m,u,0,0);}}}}catch(_o){{}}}})({},{});",
                    msg_lit, url_lit
                );
                // Best-effort: attempt to invoke handler; log if it fails
                if let Some(code2) = V8String::new(tc, &call_onerror) {
                    let undefined2: Local<Value> = undefined(tc).into();
                    let origin2 = ScriptOrigin::new(
                        tc,
                        name.into(),
                        0,
                        0,
                        false,
                        0,
                        undefined2,
                        false,
                        false,
                        false,
                    );
                    if Script::compile(tc, code2, Some(&origin2))
                        .and_then(|s| s.run(tc))
                        .is_none()
                    {
                        Console::info("window.onerror dispatch failed");
                    }
                }
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
        self.run_script_internal(prelude, "valor://runtime_prelude")?;
        self.stubs_installed = true;
        Ok(())
    }
}

impl V8Engine {
    /// Convert a generic `JSValue` to a V8 `Local<Value>`.
    fn from_js_value<'s>(
        scope: &mut HandleScope<'s, ()>,
        value: &JSValue,
    ) -> Local<'s, Value> {
        match value {
            JSValue::Undefined => undefined(scope).into(),
            JSValue::Null => null(scope).into(),
            JSValue::Boolean(b) => Boolean::new(scope, *b).into(),
            JSValue::Number(n) => Number::new(scope, *n).into(),
            JSValue::String(s) => V8String::new(scope, s.as_str()).unwrap().into(),
        }
    }

    /// Wrap a `HostFnKind` as a V8 `Function`.
    fn make_v8_callback<'s>(
        scope: &mut HandleScope<'s>,
        host_context: HostContext,
        host_fn: HostFnKind,
    ) -> Local<'s, Function> {
        // Allocate payload and leak it; V8 has no finalizer hook here. In practice this lives as long as the function.
        let payload = Box::new((host_context, host_fn));
        let ptr = Box::into_raw(payload) as *mut c_void;
        let external = External::new(scope, ptr);
        Function::builder(host_fn_dispatch)
            .data(external.into())
            .build(scope)
            .unwrap()
    }

    /// Install host bindings (namespaces and functions) onto the global object.
    pub fn install_bindings(
        &mut self,
        host_context: HostContext,
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
            let ns_key = V8String::new(scope, namespace_name).unwrap();
            let existing = global.get(scope, ns_key.into());
            let target_obj: Local<Object> = if let Some(val) =
                existing.and_then(|v| Local::<Object>::try_from(v).ok())
            {
                val
            } else {
                let obj = Object::new(scope);
                let _ = global.set(scope, ns_key.into(), obj.into());
                obj
            };

            // Install properties
            for (property_name, property_value) in &namespace.properties {
                let key = V8String::new(scope, property_name).unwrap();
                let value = Self::from_js_value(scope, property_value);
                let _ = target_obj.set(scope, key.into(), value);
            }

            // Install functions
            for (function_name, function_kind) in &namespace.functions {
                let function =
                    Self::make_v8_callback(scope, host_context.clone(), function_kind.clone());
                let key = V8String::new(scope, function_name).unwrap();
                let _ = target_obj.set(scope, key.into(), function.into());
                // Expose host functions under __valorHost_* for the runtime wrapper to call directly.
                if *namespace_name == "document" {
                    let host_alias = format!("__valorHost_{}", function_name);
                    if let Some(alias_key) = V8String::new(scope, &host_alias) {
                        let _ = target_obj.set(scope, alias_key.into(), function.into());
                    }
                }
            }
        }
        Ok(())
    }
}

impl JsEngine for V8Engine {
    fn eval_script(&mut self, source: &str, url: &str) -> Result<()> {
        // Ensure stubs exist so basic console/document calls don't throw
        self.ensure_stubs()?;
        self.run_script_internal(source, url)
    }

    fn eval_module(&mut self, source: &str, url: &str) -> Result<()> {
        // Minimal module support for Phase 6: evaluate pre-bundled side-effect code
        // using the classic script path. A future iteration can switch to true V8
        // module compilation and instantiation once the static import graph is wired.
        self.ensure_stubs()?;
        self.run_script_internal(source, url)
    }

    fn run_jobs(&mut self) -> Result<()> {
        // V8 runs microtasks at checkpoints; perform within a context and catch exceptions.
        if let Some(isolate_container) = &mut self.isolate {
            let isolate = &mut isolate_container.isolate;
            let hs = &mut HandleScope::new(isolate);
            let global_context = self
                .inner
                .as_ref()
                .ok_or_else(|| anyhow!("context not initialized"))?;
            let local_context: Local<Context> = Local::new(hs, global_context);
            let cs = &mut ContextScope::new(hs, local_context);
            let tc = &mut TryCatch::new(cs);
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
                Console::exception(message.clone(), stack.as_deref());
                // Dispatch to window.onunhandledrejection if present
                fn escape_js(s: &str) -> String {
                    let mut out = String::with_capacity(s.len() + 8);
                    for ch in s.chars() {
                        match ch {
                            '\\' => out.push_str("\\\\"),
                            '"' => out.push_str("\\\""),
                            '\n' => out.push_str("\\n"),
                            '\r' => out.push_str("\\r"),
                            _ => out.push(ch),
                        }
                    }
                    out
                }
                let msg_lit = format!("\"{}\"", escape_js(&message));
                let call_unhandled = format!(
                    "(function(m){{try{{if(typeof window!=='undefined'&&typeof window.onunhandledrejection==='function'){{window.onunhandledrejection({{type:'unhandledrejection', reason:m}});}}}}catch(_ ){{}}}})({});",
                    msg_lit
                );
                if let Some(code2) = V8String::new(tc, &call_unhandled) {
                    let origin2_name = V8String::new(tc, "valor://unhandledrejection").unwrap();
                    let undefined2: Local<Value> = undefined(tc).into();
                    let origin2 = ScriptOrigin::new(
                        tc,
                        origin2_name.into(),
                        0,
                        0,
                        false,
                        0,
                        undefined2,
                        false,
                        false,
                        false,
                    );
                    let _ = Script::compile(tc, code2, Some(&origin2)).and_then(|s| s.run(tc));
                }
                // Also notify window.onerror for visibility, mirroring classic script errors
                let msg_lit = format!("\"{}\"", escape_js(&message));
                let url_lit = "\"valor://microtask\"".to_string();
                let call_onerror = format!(
                    "(function(m,u){{try{{if(typeof window!=='undefined'&&typeof window.onerror==='function'){{window.onerror(m,u,0,0);}}}}catch(_o){{}}}})({},{});",
                    msg_lit, url_lit
                );
                if let Some(code3) = V8String::new(tc, &call_onerror) {
                    let undefined3: Local<Value> = undefined(tc).into();
                    let origin3_name = V8String::new(tc, "valor://microtask").unwrap();
                    let origin3 = ScriptOrigin::new(
                        tc,
                        origin3_name.into(),
                        0,
                        0,
                        false,
                        0,
                        undefined3,
                        false,
                        false,
                        false,
                    );
                    let _ = Script::compile(tc, code3, Some(&origin3)).and_then(|s| s.run(tc));
                }
                // Do not propagate as a hard error; browsers don't crash on unhandled rejections.
                // We already logged to console and invoked the handler if any.
            }
        }
        Ok(())
    }
}
