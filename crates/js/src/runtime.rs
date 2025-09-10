//! Engine-agnostic JavaScript runtime prelude.
//! 
//! This module exports a small JavaScript snippet that establishes minimal
//! globals and browser-like conveniences used by Valor during tests:
//! - Ensures `window` and `document` exist.
//! - Implements `document.addEventListener` and a hidden listener store.
//! - Provides `document.__valorDispatchDOMContentLoaded` to fire the event.
//! - Wraps `document.getElementById` so it returns lightweight handle objects
//!   whose `textContent` setter calls the host `document.setTextContent`.
//!
//! The prelude is pure JS and can be evaluated in any engine; the rusty engine
//! must not embed behavior beyond evaluating this prelude and installing host
//! bindings.

/// Return the JavaScript source for the runtime prelude.
///
/// Engines should evaluate this once per context before running page scripts.
pub fn runtime_prelude() -> &'static str {
    // Note: keep this idempotent; guards and hidden markers prevent double work.
    r#"
    (function(){
      // window/document shims
      if (typeof globalThis.window === 'undefined') { globalThis.window = globalThis; }
      if (typeof globalThis.document === 'undefined') { globalThis.document = {}; }
      var d = globalThis.document;
      if (!d || typeof d !== 'object') { return; }

      // Event listener storage
      if (typeof d.__valorListeners === 'undefined') {
        Object.defineProperty(d, '__valorListeners', { value: Object.create(null), enumerable: false, writable: true });
      }
      if (typeof d.__domContentLoadedFired === 'undefined') {
        Object.defineProperty(d, '__domContentLoadedFired', { value: false, enumerable: false, writable: true });
      }

      // document.addEventListener(type, listener)
      if (typeof d.addEventListener !== 'function') {
        d.addEventListener = function(type, listener) {
          if (!type || typeof listener !== 'function') { return; }
          var list = d.__valorListeners[type];
          if (!Array.isArray(list)) { list = d.__valorListeners[type] = []; }
          list.push(listener);
          if (type === 'DOMContentLoaded' && d.__domContentLoadedFired) {
            try { listener(); } catch (_) {}
          }
        };
      }

      // document.__valorDispatchDOMContentLoaded()
      if (typeof d.__valorDispatchDOMContentLoaded !== 'function') {
        d.__valorDispatchDOMContentLoaded = function() {
          if (d.__domContentLoadedFired) { return; }
          d.__domContentLoadedFired = true;
          var list = d.__valorListeners && d.__valorListeners['DOMContentLoaded'];
          if (Array.isArray(list)) {
            for (var i = 0; i < list.length; i++) {
              var fn = list[i];
              try { if (typeof fn === 'function') fn(); } catch (_) {}
            }
          }
        };
      }

      // document.getElementById: ensure exists (host will provide actual impl)
      if (typeof d.getElementById !== 'function') {
        d.getElementById = function(){ return null; };
      }

      // Handle factory used by getElementById wrapper
      if (typeof d.__valorMakeHandle !== 'function') {
        d.__valorMakeHandle = function(key) {
          var skey = (typeof key === 'string') ? key : String(key);
          var o = {};
          Object.defineProperty(o, '__nodeKey', { value: skey, enumerable: false, configurable: false, writable: false });
          Object.defineProperty(o, 'textContent', {
            get: function(){
              try {
                if (typeof d.getTextContent === 'function') {
                  var val = d.getTextContent(skey);
                  return (val == null) ? '' : String(val);
                }
              } catch (_) {}
              return '';
            },
            set: function(v){ try { if (typeof d.setTextContent === 'function') d.setTextContent(skey, String(v)); } catch(_){} },
            enumerable: true,
            configurable: true
          });
          return o;
        };
      }

      // Wrap getElementById to return a handle when the host returns a string NodeKey.
      (function(){
        var original = d.getElementById;
        if (typeof original === 'function' && !original.__valorWrapped) {
          // Prefer the host-provided resolver if exposed, fallback to the current function.
          var wrapped = function(id){
            try {
              var host = d.__valorHost_getElementById;
              var fn = (typeof host === 'function') ? host : original;
              var k = fn.call(d, id);
              if (typeof k === 'string') return d.__valorMakeHandle(k);
              return k == null ? null : k;
            } catch (_) {
              return null;
            }
          };
          Object.defineProperty(wrapped, '__valorWrapped', { value: true, enumerable: false });
          d.getElementById = wrapped;
        }
      })();
    })();
    "#
}
