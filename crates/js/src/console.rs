//! Simple stdout-backed console for the JS facade.
//!
//! This module centralizes output from JavaScript console methods and engine
//! error reporting. For now, it prints using the `log` crate.

use crate::bindings::{HostLogger, LogLevel};
use log::{error, info, warn};

/// Console provides helper functions to print messages emitted by the JS
/// runtime and the engine itself. This keeps output routing in one place so it
/// can later be swapped to a different backend.
pub struct Console;

impl Console {
    /// Print a generic log line to stdout.
    #[inline]
    pub fn log<M: AsRef<str>>(message: M) {
        info!("[JS]: {}", message.as_ref());
    }

    /// Print an informational line to stdout.
    #[inline]
    pub fn info<M: AsRef<str>>(message: M) {
        info!("[JS]: {}", message.as_ref());
    }

    /// Print a warning line to stdout.
    #[inline]
    pub fn warn<M: AsRef<str>>(message: M) {
        warn!("[JS]: {}", message.as_ref());
    }

    /// Print an error line to stdout.
    #[inline]
    pub fn error<M: AsRef<str>>(message: M) {
        error!("[JS]: {}", message.as_ref());
    }

    /// Print an exception with optional stack trace to stdout.
    #[inline]
    pub fn exception<M: AsRef<str>>(message: M, stack: Option<&str>) {
        match stack {
            Some(stack_trace) if !stack_trace.is_empty() => {
                error!("[JS]: {}\n{}", message.as_ref(), stack_trace);
            }
            _ => {
                error!("[JS]: {}", message.as_ref());
            }
        }
    }
}

/// A `HostLogger` implementation that routes to the `Console` helpers.
pub struct ConsoleLogger;

impl HostLogger for ConsoleLogger {
    #[inline]
    fn log(&self, level: LogLevel, message: &str) {
        match level {
            LogLevel::Trace | LogLevel::Debug | LogLevel::Info => Console::info(message),
            LogLevel::Warn => Console::warn(message),
            LogLevel::Error => Console::error(message),
        }
    }
}

/// Return a small JavaScript snippet that defines a console object which forwards
/// calls to the host via `__valor_host_post`. The message protocol is:
///   `console|<level>|<joined-args>`
/// where `<level>` is one of log, info, warn, error and `<joined-args>` is a space-
/// joined `String()` representation of the arguments.
#[inline]
pub const fn console_shim_js() -> &'static str {
    "
    (function(){
      if (typeof globalThis.console === 'undefined') {
        globalThis.console = {};
      }
      var forward = function(level, args){
        try {
          var parts = [];
          for (var i = 0; i < args.length; i++) { parts.push(String(args[i])); }
          var line = parts.join(' ');
          if (typeof globalThis.__valor_host_post === 'function') {
            globalThis.__valor_host_post('console|' + level + '|' + line);
          }
        } catch (_) { /* ignore */ }
      };
      var define = function(name){
        if (typeof globalThis.console[name] !== 'function') {
          globalThis.console[name] = function(){ forward(name, arguments); };
        }
      };
      define('log');
      define('info');
      define('warn');
      define('error');
    })();
    "
}
