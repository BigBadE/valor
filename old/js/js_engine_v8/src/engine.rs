//! V8 JavaScript engine implementation.

use crate::bindings::install_bindings_impl;
use crate::conversions::build_onerror_script;
use anyhow::{Result, anyhow};
use core::pin::Pin;
use js::Console;
use js::runtime::RUNTIME_PRELUDE;
use js::{HostBindings, HostContext, JsEngine};
use std::collections::HashMap;
use std::sync::Once;
use v8::{
    Context, ContextScope, CreateParams, Global, Isolate, Local, Module, OwnedIsolate, Script,
    ScriptOrigin, String as V8String, V8, new_default_platform,
};

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

        let result = install_bindings_impl(isolate_mut, &self.context, host_context, bindings);

        // SAFETY: Exit the isolate after we're done using it
        unsafe { isolate_mut.exit() };

        result
    }

    fn run_jobs_impl(isolate_mut: &mut Isolate, context: &Global<Context>) -> Result<()> {
        v8::scope!(let scope, isolate_mut);

        let local_context: Local<Context> = Local::new(scope, context);
        let scope = &mut ContextScope::new(scope, local_context);

        v8::tc_scope!(let tc, scope);

        tc.perform_microtask_checkpoint();
        Ok(())
    }
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
