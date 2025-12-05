//! JavaScript engine adapter using V8 backend.
//!
//! This crate provides a V8-backed implementation of the `JsEngine` trait.

mod real {
    //! Minimal V8-based JavaScript engine implementation for the Valor browser.
    //!
    //! This crate provides a narrow, swappable interface to a JavaScript engine with
    //! a V8-backed implementation that is always enabled in this build.

    use anyhow::{Result, anyhow};
    use core::convert::TryFrom as _;
    use core::ffi::c_void;
    use core::pin::Pin;
    use js::Console;
    use js::runtime::RUNTIME_PRELUDE;
    use js::{HostBindings, HostContext, HostFnKind, HostNamespace, JSValue, JsEngine};
    use std::collections::HashMap;
    use std::sync::Once;
    use v8::{
        Boolean, Context, ContextScope, CreateParams, External, Function,
        FunctionCallbackArguments, Global, Isolate, Local, Module, Number, Object, OwnedIsolate,
        ReturnValue, Script, ScriptOrigin, String as V8String, V8, Value, new_default_platform,
    };

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
    fn host_fn_dispatch<'s, 'i>(
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
        let payload: &(HostContext, HostFnKind) =
            unsafe { &*ptr.cast::<(HostContext, HostFnKind)>() };
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

    /// Collect V8 callback arguments into engine-agnostic `JSValue`s.
    #[inline]
    fn collect_js_args<'s, 'i>(
        scope: &mut v8::PinScope<'s, 'i>,
        args: &FunctionCallbackArguments,
    ) -> Vec<JSValue> {
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
    fn jsvalue_to_local<'s, 'i>(
        scope: &mut v8::PinScope<'s, 'i>,
        value: JSValue,
    ) -> Local<'s, Value> {
        match value {
            JSValue::Undefined => v8::undefined(scope).into(),
            JSValue::Null => v8::null(scope).into(),
            JSValue::Boolean(boolean_value) => Boolean::new(scope, boolean_value).into(),
            JSValue::Number(number_value) => Number::new(scope, number_value).into(),
            JSValue::String(string_value) => V8String::new(scope, string_value.as_str())
                .map_or_else(|| v8::undefined(scope).into(), Into::into),
        }
    }

    /// V8-backed engine, always compiled.
    pub struct V8Engine {
        /// The owned isolate (Pin<Box> ensures it doesn't move in memory).
        /// Must be first so it's initialized before context.
        isolate: Pin<Box<OwnedIsolate>>,
        /// The current global V8 context.
        context: Global<Context>,
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
            // Initialize V8 platform (singleton per-process).
            static START: Once = Once::new();
            START.call_once(|| {
                let platform = new_default_platform(0, false).make_shared();
                V8::initialize_platform(platform);
                V8::initialize();
            });

            // Create isolate using the global platform initialized above
            // Pin<Box> ensures it doesn't move in memory (Global<Context> stores raw pointer to isolate)
            let mut isolate = Box::pin(Isolate::new(CreateParams::default()));
            let context = {
                // SAFETY: We're pinning the isolate, so it's safe to create a mutable reference
                let isolate_mut = unsafe { isolate.as_mut().get_unchecked_mut() };
                v8::scope!(let scope, isolate_mut);
                let ctx: Local<Context> = Context::new(scope, Default::default());
                Global::new(scope, ctx)
            };
            Console::info("V8Engine initialized");
            Ok(Self {
                isolate,
                context,
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
            // SAFETY: Isolate is pinned. We need to re-enter it because another isolate
            // might have been entered since this engine was created (multiple V8Engine instances)
            let isolate_mut = unsafe { self.isolate.as_mut().get_unchecked_mut() };
            unsafe { isolate_mut.enter() };

            let result = Self::run_script_internal_impl(isolate_mut, &self.context, source, url);

            // SAFETY: Exit the isolate after we're done using it
            unsafe { isolate_mut.exit() };

            result
        }

        fn run_script_internal_impl(
            isolate_mut: &mut Isolate,
            context: &Global<Context>,
            source: &str,
            url: &str,
        ) -> Result<()> {
            v8::scope!(let scope, isolate_mut);

            let local_context: Local<Context> = Local::new(scope, context);
            let scope = &mut ContextScope::new(scope, local_context);

            v8::tc_scope!(let tc, scope);

            let code = V8String::new(tc, source).ok_or_else(|| anyhow!("alloc v8 string"))?;
            let name = V8String::new(tc, url).ok_or_else(|| anyhow!("alloc v8 name"))?;
            let origin = ScriptOrigin::new(
                tc,
                name.into(),
                0,
                0,
                false,
                0,
                None,
                false,
                false,
                false,
                None,
            );
            let failed = Script::compile(tc, code, Some(&origin))
                .is_some_and(|compiled| compiled.run(tc).is_none());
            if !failed {
                return Ok(());
            }
            if !tc.has_caught() {
                return Err(anyhow!("v8 failed"));
            }
            let exc = tc.exception();
            let exc_str = exc.and_then(|val| val.to_string(tc)).map_or_else(
                || "Uncaught exception".to_owned(),
                |val| val.to_rust_string_lossy(tc),
            );
            let stack = tc
                .stack_trace()
                .and_then(|val| val.to_string(tc))
                .map(|val| val.to_rust_string_lossy(tc));
            let message = tc.message().map_or_else(
                || exc_str.clone(),
                |msg_obj| msg_obj.get(tc).to_rust_string_lossy(tc),
            );
            Console::exception(message.clone(), stack.as_deref());
            let call_onerror = build_onerror_script(&message, url);
            if let Some(code2) = V8String::new(tc, &call_onerror) {
                let origin2 = ScriptOrigin::new(
                    tc,
                    name.into(),
                    0,
                    0,
                    false,
                    0,
                    None,
                    false,
                    false,
                    false,
                    None,
                );
                if let Some(compiled2) = Script::compile(tc, code2, Some(&origin2))
                    && compiled2.run(tc).is_none()
                {
                    Console::info("window.onerror dispatch failed");
                }
            }
            Err(anyhow!("v8 failed"))
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
            let prelude = RUNTIME_PRELUDE;
            self.run_script_internal(prelude, "valor://runtime_prelude")?;
            self.stubs_installed = true;
            Ok(())
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
        fn make_v8_callback<'s, 'i>(
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
            // SAFETY: Isolate is pinned. We need to re-enter it because another isolate
            // might have been entered since this engine was created (multiple V8Engine instances)
            let isolate_mut = unsafe { self.isolate.as_mut().get_unchecked_mut() };
            unsafe { isolate_mut.enter() };

            let result =
                Self::install_bindings_impl(isolate_mut, &self.context, host_context, bindings);

            // SAFETY: Exit the isolate after we're done using it
            unsafe { isolate_mut.exit() };

            result
        }

        fn install_bindings_impl(
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

                Self::install_namespace_properties(scope, namespace, target_obj);
                Self::install_namespace_functions(
                    scope,
                    host_context,
                    namespace_name,
                    namespace,
                    target_obj,
                );
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
                let value = Self::from_js_value(scope, property_value);
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
                let Some(function) =
                    Self::make_v8_callback(scope, host_context, function_kind.clone())
                else {
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
        fn from_js_value<'s, 'i>(
            scope: &mut v8::PinScope<'s, 'i>,
            value: &JSValue,
        ) -> Local<'s, Value> {
            // Avoid pattern matching on a reference to satisfy clippy pedantic.
            jsvalue_to_local(scope, value.clone())
        }

        fn run_jobs_impl(isolate_mut: &mut Isolate, context: &Global<Context>) -> Result<()> {
            v8::scope!(let scope, isolate_mut);

            let local_context: Local<Context> = Local::new(scope, context);
            let scope = &mut ContextScope::new(scope, local_context);

            v8::tc_scope!(let tc, scope);

            tc.perform_microtask_checkpoint();
            Ok(())
        }

        // Trait impl lives below inside this module
    }

    impl JsEngine for V8Engine {
        #[inline]
        fn eval_script(&mut self, source: &str, url: &str) -> Result<()> {
            self.ensure_stubs()?;
            self.run_script_internal(source, url)
        }

        #[inline]
        fn eval_module(&mut self, source: &str, url: &str) -> Result<()> {
            self.ensure_stubs()?;
            self.run_script_internal(source, url)
        }

        #[inline]
        fn run_jobs(&mut self) -> Result<()> {
            // SAFETY: Isolate is pinned. We need to re-enter it because another isolate
            // might have been entered since this engine was created (multiple V8Engine instances)
            let isolate_mut = unsafe { self.isolate.as_mut().get_unchecked_mut() };
            unsafe { isolate_mut.enter() };

            let result = Self::run_jobs_impl(isolate_mut, &self.context);

            // SAFETY: Exit the isolate after we're done using it
            unsafe { isolate_mut.exit() };

            result
        }
    }
}

pub use real::V8Engine;
