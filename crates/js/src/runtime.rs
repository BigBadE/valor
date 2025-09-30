//! Engine-agnostic JavaScript runtime prelude.
//!
//! This module exports a small JavaScript snippet that establishes minimal
//! globals and browser-like conveniences used by Valor during tests:
//! - Ensures `window` and `document` exist.
//! - Implements `document.addEventListener` and a hidden listener store.
//! - Provides `document.__valorDispatchDOMContentLoaded` to fire the event.
//! - Wraps `document.getElementById` so it returns lightweight handle objects
//!   whose `textContent` setter calls the host `document.setTextContent`.
//! - Implements timers: `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval`.
//!   Host drives time with `globalThis.__valorTickTimersOnce(nowMs)` which executes
//!   at most one due timer callback per call. This keeps ordering simple and lets
//!   the host flush microtasks between callbacks if desired.
//!
//! The prelude is pure JS and can be evaluated in any engine; the rusty engine
//! must not embed behavior beyond evaluating this prelude and installing host
//! bindings.

/// JavaScript source for the runtime prelude.
///
/// Engines should evaluate this once per context before running page scripts.
///
/// Note: keep this idempotent; guards and hidden markers prevent double work.
pub const RUNTIME_PRELUDE: &str = r"
    (function(){
      // window/document shims
      if (typeof globalThis.window === 'undefined') { globalThis.window = globalThis; }
      if (typeof globalThis.document === 'undefined') { globalThis.document = {}; }
      var d = globalThis.document;
      if (!d || typeof d !== 'object') { return; }

      // Event listener storage (bubble and capture)
      if (typeof d.__valorListeners === 'undefined') {
        Object.defineProperty(d, '__valorListeners', { value: Object.create(null), enumerable: false, writable: true });
      }
      if (typeof d.__valorListenersCapture === 'undefined') {
        Object.defineProperty(d, '__valorListenersCapture', { value: Object.create(null), enumerable: false, writable: true });
      }
      // Per-node listener storage keyed by NodeKey (bubble and capture)
      if (typeof d.__valorNodeListeners === 'undefined') {
        Object.defineProperty(d, '__valorNodeListeners', { value: Object.create(null), enumerable: false, writable: true });
      }
      if (typeof d.__valorNodeListenersCapture === 'undefined') {
        Object.defineProperty(d, '__valorNodeListenersCapture', { value: Object.create(null), enumerable: false, writable: true });
      }
      if (typeof d.__domContentLoadedFired === 'undefined') {
        Object.defineProperty(d, '__domContentLoadedFired', { value: false, enumerable: false, writable: true });
      }

      // Host -> JS event dispatch bridge. The host calls this to inject an event.
      if (typeof d.__valorHostDispatch !== 'function') {
        d.__valorHostDispatch = function(type, targetKey, props) {
          try {
            var evt = { type: String(type), bubbles: !!(props && props.bubbles), cancelable: !!(props && props.cancelable) };
            // Pointer/Mouse coordinates/buttons/modifiers
            var fields = ['clientX','clientY','button','buttons','detail','ctrlKey','shiftKey','altKey','metaKey','key','code','repeat'];
            for (var i = 0; i < fields.length; i++) { var f = fields[i]; if (props && Object.prototype.hasOwnProperty.call(props, f)) evt[f] = props[f]; }
            var tgt = (targetKey != null && d.__valorMakeHandle) ? d.__valorMakeHandle(String(targetKey)) : d;
            evt.target = tgt;
            return d.dispatchEvent(evt);
          } catch (_) { return false; }
        };
      }

      // document.addEventListener(type, listener, options)
      if (typeof d.addEventListener !== 'function') {
        d.addEventListener = function(type, listener, options) {
          if (!type || typeof listener !== 'function') { return; }
          var useCapture = false;
          if (options === true) useCapture = true; else if (options && typeof options === 'object' && !!options.capture) useCapture = true;
          var bag = useCapture ? d.__valorListenersCapture : d.__valorListeners;
          var list = bag[type];
          if (!Array.isArray(list)) { list = bag[type] = []; }
          list.push(listener);
          if (type === 'DOMContentLoaded' && d.__domContentLoadedFired) {
            try { listener(); } catch (_) {}
          }
        };
      }

      // document.dispatchEvent(event) for synthetic events
      if (typeof d.dispatchEvent !== 'function') {
        d.dispatchEvent = function(event) {
          if (!event || !event.type) { return false; }
          var type = String(event.type);
          var listCap = d.__valorListenersCapture && d.__valorListenersCapture[type];
          var list = d.__valorListeners && d.__valorListeners[type];
          // Determine target: document by default, or a handle if provided
          var target = (event && event.target && event.target.__nodeKey) ? event.target : d;
          var targetKey = (target && target.__nodeKey) ? String(target.__nodeKey) : null;
          // Install Event-like helpers if not present
          if (typeof event.defaultPrevented !== 'boolean') { event.defaultPrevented = false; }
          if (typeof event.cancelBubble !== 'boolean') { event.cancelBubble = false; }
          event.__stop = false; event.__stopImmediate = false;
          if (typeof event.preventDefault !== 'function') { event.preventDefault = function(){ this.defaultPrevented = true; }; }
          if (typeof event.stopPropagation !== 'function') { event.stopPropagation = function(){ this.__stop = true; this.cancelBubble = true; }; }
          if (typeof event.stopImmediatePropagation !== 'function') { event.stopImmediatePropagation = function(){ this.__stop = true; this.__stopImmediate = true; this.cancelBubble = true; }; }
          event.target = target;

          // Build ancestor chain from document to target parent using host hook if available
          var ancestors = [];
          if (targetKey && typeof d.__valorHost_getParentKey === 'function') {
            var p = String(d.__valorHost_getParentKey(targetKey) || '');
            var guard = 0;
            while (p && guard++ < 10000) { ancestors.push(p); p = String(d.__valorHost_getParentKey(p) || ''); }
            ancestors.reverse(); // from outermost to innermost
          }

          function callList(ctx, lst) {
            if (!Array.isArray(lst)) return;
            for (var i = 0; i < lst.length; i++) {
              var fn = lst[i];
              try { if (typeof fn === 'function') fn.call(ctx, event); } catch(_) {}
              if (event.__stopImmediate) break;
            }
          }

          // CAPTURING_PHASE = 1, AT_TARGET = 2, BUBBLING_PHASE = 3 if Event exists
          var CAP = (globalThis.Event && globalThis.Event.CAPTURING_PHASE) ? globalThis.Event.CAPTURING_PHASE : 1;
          var AT = (globalThis.Event && globalThis.Event.AT_TARGET) ? globalThis.Event.AT_TARGET : 2;
          var BUB = (globalThis.Event && globalThis.Event.BUBBLING_PHASE) ? globalThis.Event.BUBBLING_PHASE : 3;
          var bubbles = !!event.bubbles;
          // 1) Capture: document capture, then ancestor capture
          event.eventPhase = CAP;
          event.currentTarget = d;
          callList(d, listCap);
          if (!event.__stop && ancestors.length) {
            for (var ai = 0; ai < ancestors.length; ai++) {
              var ak = ancestors[ai];
              var ncap = d.__valorNodeListenersCapture[ak] && d.__valorNodeListenersCapture[ak][type];
              event.currentTarget = d.__valorMakeHandle ? d.__valorMakeHandle(ak) : { __nodeKey: ak };
              callList(event.currentTarget, ncap);
              if (event.__stop) break;
            }
          }
          if (event.__stop) return !event.defaultPrevented;
          // At target: if node handle, invoke node listeners; otherwise document listeners at target.
          event.eventPhase = AT;
          event.currentTarget = target;
          if (targetKey) {
            var ncap = d.__valorNodeListenersCapture[targetKey] && d.__valorNodeListenersCapture[targetKey][type];
            callList(target, ncap);
            if (event.__stop) return !event.defaultPrevented;
            var nbub = d.__valorNodeListeners[targetKey] && d.__valorNodeListeners[targetKey][type];
            callList(target, nbub);
          } else {
            event.currentTarget = d;
            callList(d, list);
          }
          if (event.__stop) return !event.defaultPrevented;
          // 3) Bubble: ancestors from innermost to outermost, then document bubble
          if (bubbles) {
            event.eventPhase = BUB;
            if (ancestors.length && !event.__stop) {
              for (var bi = ancestors.length - 1; bi >= 0; bi--) {
                var bk = ancestors[bi];
                var bb = d.__valorNodeListeners[bk] && d.__valorNodeListeners[bk][type];
                event.currentTarget = d.__valorMakeHandle ? d.__valorMakeHandle(bk) : { __nodeKey: bk };
                callList(event.currentTarget, bb);
                if (event.__stop) break;
              }
            }
            if (!event.__stop && Array.isArray(list)) {
              event.currentTarget = d;
              callList(d, list);
            }
          }
          return !event.defaultPrevented;
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

      // Utilities
      function __splitKeysToHandles(s) {
        if (typeof s !== 'string' || s.length === 0) return [];
        var parts = s.trim().split(/\s+/);
        var out = [];
        for (var i = 0; i < parts.length; i++) {
          var p = parts[i];
          if (p) out.push(d.__valorMakeHandle(p));
        }
        return out;
      }

      // Enhance handle factory with core DOM APIs
      if (!d.__valorHandleAugmented) {
        d.__valorHandleAugmented = true;
        var make = d.__valorMakeHandle;
        d.__valorMakeHandle = function(key) {
          var o = make(key);
          // Per-node addEventListener/removeEventListener
          o.addEventListener = function(type, listener, options) {
            var useCapture = false;
            if (options === true) useCapture = true; else if (options && typeof options === 'object' && !!options.capture) useCapture = true;
            var bag = useCapture ? d.__valorNodeListenersCapture : d.__valorNodeListeners;
            var byTypeBag = bag[o.__nodeKey] || (bag[o.__nodeKey] = Object.create(null));
            var list = byTypeBag[type] || (byTypeBag[type] = []);
            list.push(listener);
          };
          // Node-targeted dispatch; normalize target and forward to document
          o.dispatchEvent = function(evt) {
            evt = evt || {}; evt.target = o; return d.dispatchEvent(evt);
          };
          o.removeEventListener = function(type, listener, options) {
            var useCapture = false;
            if (options === true) useCapture = true; else if (options && typeof options === 'object' && !!options.capture) useCapture = true;
            var bag = useCapture ? d.__valorNodeListenersCapture : d.__valorNodeListeners;
            var byTypeBag = bag[o.__nodeKey]; if (!byTypeBag) return;
            var list = byTypeBag[type]; if (!Array.isArray(list)) return;
            for (var i = 0; i < list.length; i++) { if (list[i] === listener) { list.splice(i,1); break; } }
          };
          // Element methods (assume element; text-only handles created via createTextNode set a marker)
          o.appendChild = function(child) {
            if (!child) return null;
            if (child.__isFragment) {
              var list = child.__childKeys || [];
              for (var i = 0; i < list.length; i++) {
                var ck = list[i];
                var prev = String(d.__valorHost_getParentKey(ck) || '');
                d.__valorHost_appendChild(this.__nodeKey, ck);
                if (prev && prev !== this.__nodeKey) { try { d.__valorEnqueueMutation('childList', prev, { removed: [ck] }); } catch(_){} }
                try { d.__valorEnqueueMutation('childList', this.__nodeKey, { added: [ck] }); } catch(_){}
              }
              return child;
            }
            if (child.__nodeKey) {
              var prevParent = String(d.__valorHost_getParentKey(child.__nodeKey) || '');
              d.__valorHost_appendChild(this.__nodeKey, child.__nodeKey);
              if (prevParent && prevParent !== this.__nodeKey) { try { d.__valorEnqueueMutation('childList', prevParent, { removed: [child.__nodeKey] }); } catch(_){} }
              try { d.__valorEnqueueMutation('childList', this.__nodeKey, { added: [child.__nodeKey] }); } catch(_){}
              return child;
            }
            return null;
          };
          o.insertBefore = function(newNode, referenceNode) {
            if (!newNode) return null;
            if (!referenceNode || !referenceNode.__nodeKey) { return this.appendChild(newNode); }
            var idx = Number(d.__valorHost_getChildIndex(this.__nodeKey, referenceNode.__nodeKey));
            if (isFinite(idx) && idx >= 0) {
              var prevParent = String(d.__valorHost_getParentKey(newNode.__nodeKey) || '');
              d.__valorHost_appendChild(this.__nodeKey, newNode.__nodeKey, idx);
              if (prevParent && prevParent !== this.__nodeKey) { try { d.__valorEnqueueMutation('childList', prevParent, { removed: [newNode.__nodeKey] }); } catch(_){} }
              try { d.__valorEnqueueMutation('childList', this.__nodeKey, { added: [newNode.__nodeKey] }); } catch(_){}
            } else {
              this.appendChild(newNode);
            }
            return newNode;
          };
          o.removeChild = function(child) {
            if (child && child.__nodeKey) {
              var pk = this.__nodeKey;
              d.__valorHost_removeNode(child.__nodeKey);
              try { d.__valorEnqueueMutation('childList', pk, { removed: [child.__nodeKey] }); } catch(_){}
            }
            return child;
          };
          o.replaceChild = function(newChild, oldChild) {
            if (!oldChild || !oldChild.__nodeKey) return null;
            var idx = Number(d.__valorHost_getChildIndex(this.__nodeKey, oldChild.__nodeKey));
            // Remove old child and enqueue
            this.removeChild(oldChild);
            if (newChild && newChild.__nodeKey) {
              if (isFinite(idx) && idx >= 0) {
                var prevParent = String(d.__valorHost_getParentKey(newChild.__nodeKey) || '');
                d.__valorHost_appendChild(this.__nodeKey, newChild.__nodeKey, idx);
                if (prevParent && prevParent !== this.__nodeKey) { try { d.__valorEnqueueMutation('childList', prevParent, { removed: [newChild.__nodeKey] }); } catch(_){} }
                try { d.__valorEnqueueMutation('childList', this.__nodeKey, { added: [newChild.__nodeKey] }); } catch(_){}
              } else {
                this.appendChild(newChild);
              }
              return newChild;
            }
            return null;
          };
          o.setAttribute = function(name, value) {
            var n = String(name);
            if (typeof o.__attrs === 'undefined') { Object.defineProperty(o, '__attrs', { value: Object.create(null), enumerable: false, writable: true }); }
            var map = o.__attrs;
            var oldVal;
            if (Object.prototype.hasOwnProperty.call(map, n)) {
              oldVal = String(map[n]);
            } else {
              var hv = d.__valorHost_getAttribute(this.__nodeKey, n);
              oldVal = (hv == null) ? '' : String(hv);
            }
            d.__valorHost_setAttribute(this.__nodeKey, n, String(value));
            map[n] = String(value);
            try { d.__valorEnqueueMutation('attributes', this.__nodeKey, { attributeName: n, oldValue: oldVal }); } catch(_){}
          };
          o.getAttribute = function(name) {
            var n = String(name);
            if (typeof o.__attrs !== 'undefined' && Object.prototype.hasOwnProperty.call(o.__attrs, n)) {
              return String(o.__attrs[n]);
            }
            var v = d.__valorHost_getAttribute(this.__nodeKey, n);
            return (v == null) ? '' : String(v);
          };
          o.removeAttribute = function(name) {
            var n = String(name);
            var oldVal;
            if (typeof o.__attrs !== 'undefined' && Object.prototype.hasOwnProperty.call(o.__attrs, n)) {
              oldVal = String(o.__attrs[n]);
            } else {
              var hv = d.__valorHost_getAttribute(this.__nodeKey, n);
              oldVal = (hv == null) ? '' : String(hv);
            }
            d.__valorHost_removeAttribute(this.__nodeKey, n);
            if (typeof o.__attrs !== 'undefined' && Object.prototype.hasOwnProperty.call(o.__attrs, n)) { delete o.__attrs[n]; }
            try { d.__valorEnqueueMutation('attributes', this.__nodeKey, { attributeName: n, oldValue: oldVal }); } catch(_){}
          };
          Object.defineProperty(o, 'id', {
            get: function(){ return o.getAttribute('id'); },
            set: function(v){ o.setAttribute('id', v); },
            enumerable: true,
          });
          Object.defineProperty(o, 'className', {
            get: function(){ return o.getAttribute('class'); },
            set: function(v){ o.setAttribute('class', v); },
            enumerable: true,
          });
          Object.defineProperty(o, 'textContent', {
            get: function(){ try { return String(d.getTextContent(this.__nodeKey) || ''); } catch(_) { return ''; } },
            set: function(v){
              try {
                var before = String(d.__valorHost_getChildrenKeys(this.__nodeKey) || '').trim().split(/\s+/).filter(function(x){return x;});
                d.setTextContent(this.__nodeKey, String(v));
                var after = String(d.__valorHost_getChildrenKeys(this.__nodeKey) || '').trim().split(/\s+/).filter(function(x){return x;});
                var removed = []; var added = [];
                var mapB = Object.create(null); for (var i=0;i<before.length;i++){ mapB[before[i]] = 1; }
                var mapA = Object.create(null); for (var i=0;i<after.length;i++){ mapA[after[i]] = 1; }
                for (var i=0;i<before.length;i++){ var k=before[i]; if (!mapA[k]) removed.push(k); }
                for (var i=0;i<after.length;i++){ var k2=after[i]; if (!mapB[k2]) added.push(k2); }
                d.__valorEnqueueMutation('childList', this.__nodeKey, { added: added, removed: removed });
              } catch(_) {}
            },
            enumerable: true,
            configurable: true
          });
          Object.defineProperty(o, 'innerText', {
            get: function(){ return o.textContent; },
            set: function(v){ o.textContent = String(v); },
            enumerable: true,
          });
          Object.defineProperty(o, 'innerHTML', {
            get: function(){ try { return String(d.__valorHost_getInnerHTML(this.__nodeKey) || ''); } catch(_) { return ''; } },
            set: function(v){
              try {
                var before = String(d.__valorHost_getChildrenKeys(this.__nodeKey) || '').trim().split(/\s+/).filter(function(x){return x;});
                d.__valorHost_setInnerHTML(this.__nodeKey, String(v));
                var after = String(d.__valorHost_getChildrenKeys(this.__nodeKey) || '').trim().split(/\s+/).filter(function(x){return x;});
                var removed = []; var added = [];
                var mapB = Object.create(null); for (var i=0;i<before.length;i++){ mapB[before[i]] = 1; }
                var mapA = Object.create(null); for (var i=0;i<after.length;i++){ mapA[after[i]] = 1; }
                for (var i=0;i<before.length;i++){ var k=before[i]; if (!mapA[k]) removed.push(k); }
                for (var i=0;i<after.length;i++){ var k2=after[i]; if (!mapB[k2]) added.push(k2); }
                d.__valorEnqueueMutation('childList', this.__nodeKey, { added: added, removed: removed });
              } catch(_) {}
            },
            enumerable: true,
          });
          // Common event helper methods
          o.click = function(){ try { var e = new MouseEvent('click', { bubbles: true, cancelable: true }); return o.dispatchEvent(e); } catch(_) { return false; } };
          o.submit = function(){ try { var e = new Event('submit', { bubbles: true, cancelable: true }); return o.dispatchEvent(e); } catch(_) { return false; } };
          // classList
          Object.defineProperty(o, 'classList', { enumerable: true, get: function(){
            var self = o;
            function tokens(){ var s = self.getAttribute('class') || ''; return s.split(/\s+/).filter(function(t){return !!t;}); }
            function setTokens(arr){ var dedup = Array.from(new Set(arr)); self.setAttribute('class', dedup.join(' ')); }
            return {
              add: function(cls){ var arr = tokens(); arr.push(String(cls)); setTokens(arr); },
              remove: function(cls){ var c = String(cls); setTokens(tokens().filter(function(t){ return t!==c; })); },
              toggle: function(cls){ var c = String(cls); var arr = tokens(); var i = arr.indexOf(c); if (i>=0){ arr.splice(i,1); setTokens(arr); return false; } else { arr.push(c); setTokens(arr); return true; } },
              contains: function(cls){ return tokens().indexOf(String(cls)) >= 0; },
              get length(){ return tokens().length; }
            };
          }});
          // dataset
          try {
            o.dataset = new Proxy({}, {
              get: function(_t, prop){ var name = String(prop); var attr = 'data-' + name.replace(/[A-Z]/g, function(m){ return '-' + m.toLowerCase(); }); return o.getAttribute(attr) || ''; },
              set: function(_t, prop, val){ var name = String(prop); var attr = 'data-' + name.replace(/[A-Z]/g, function(m){ return '-' + m.toLowerCase(); }); o.setAttribute(attr, String(val)); return true; }
            });
          } catch (_) {
            // Proxy not available; minimal fallback
            o.dataset = {};
          }
          // cloneNode
          o.cloneNode = function(deep){
            var tag = d.__valorHost_getTagName(this.__nodeKey);
            if (tag && String(tag).length > 0) {
              var c = d.createElement(tag);
              var idv = o.getAttribute('id'); if (idv) c.setAttribute('id', idv);
              var cls = o.getAttribute('class'); if (cls) c.setAttribute('class', cls);
              if (deep) {
                var kids = String(d.__valorHost_getChildrenKeys(this.__nodeKey) || '').trim();
                if (kids.length) {
                  var arr = kids.split(/\s+/);
                  for (var i=0;i<arr.length;i++) {
                    var ck = arr[i];
                    var ctag = d.__valorHost_getTagName(ck);
                    if (ctag && String(ctag).length > 0) {
                      var childHandle = d.__valorMakeHandle(ck);
                      c.appendChild(childHandle.cloneNode(true));
                    } else {
                      var t = d.getTextContent(ck);
                      c.appendChild(d.createTextNode(t));
                    }
                  }
                }
              }
              return c;
            } else {
              // text node clone
              return d.createTextNode(o.textContent || '');
            }
          };
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

      // Wrap createElement/createTextNode to return handles
      (function(){
        var ce = d.createElement; if (typeof ce === 'function' && !ce.__valorWrapped) {
          var wrapped = function(tag){ var k = d.__valorHost_createElement(tag); if (typeof k === 'string') return d.__valorMakeHandle(k); return null; };
          Object.defineProperty(wrapped, '__valorWrapped', { value: true }); d.createElement = wrapped;
        }
        var ct = d.createTextNode; if (typeof ct === 'function' && !ct.__valorWrapped) {
          var wrappedT = function(text){ var k = d.__valorHost_createTextNode(text); if (typeof k === 'string') { var h = d.__valorMakeHandle(k); Object.defineProperty(h, '__isText', { value: true }); return h; } return null; };
          Object.defineProperty(wrappedT, '__valorWrapped', { value: true }); d.createTextNode = wrappedT;
        }
      })();

      // DocumentFragment
      if (typeof d.createDocumentFragment !== 'function') {
        d.createDocumentFragment = function(){ return { __isFragment: true, __childKeys: [], appendChild: function(node){ if (!node) return null; if (node.__isFragment) { this.__childKeys = this.__childKeys.concat(node.__childKeys||[]); } else if (node.__nodeKey) { this.__childKeys.push(node.__nodeKey); } return node; } }; };
      }

      // Query wrappers returning handles/arrays
      d.getElementsByClassName = function(name){ var s = d.__valorHost_getElementsByClassName(String(name)); return __splitKeysToHandles(s); };
      d.getElementsByTagName = function(name){ var s = d.__valorHost_getElementsByTagName(String(name)); return __splitKeysToHandles(s); };
      d.querySelector = function(sel){ var k = d.__valorHost_querySelector(String(sel)); return (typeof k === 'string' && k.length>0) ? d.__valorMakeHandle(k) : null; };
      d.querySelectorAll = function(sel){ var s = d.__valorHost_querySelectorAll(String(sel)); return __splitKeysToHandles(s); };

      // =====================
      // MutationObserver (basic)
      // =====================
      if (typeof d.__moObservers === 'undefined') {
        Object.defineProperty(d, '__moObservers', { value: Object.create(null), enumerable: false, writable: true });
        Object.defineProperty(d, '__moRegByNode', { value: Object.create(null), enumerable: false, writable: true });
        Object.defineProperty(d, '__moNextId', { value: 1, enumerable: false, writable: true });
        Object.defineProperty(d, '__moDeliveryScheduled', { value: false, enumerable: false, writable: true });
      }
      function __moGetRegsFor(nodeKey) {
        var by = d.__moRegByNode;
        var list = by[nodeKey];
        if (!Array.isArray(list)) { list = by[nodeKey] = []; }
        return list;
      }
      function __moScheduleDelivery() {
        if (d.__moDeliveryScheduled) return;
        d.__moDeliveryScheduled = true;
        var qm = (typeof queueMicrotask === 'function') ? queueMicrotask : function(fn){ Promise.resolve().then(fn); };
        qm(function(){ try { d.__moDeliveryScheduled = false; d.__valorDeliverMutationRecords(); } catch(_) { d.__moDeliveryScheduled = false; } });
      }
      function __moMatches(reg, type, data, isSameTarget) {
        var opt = reg.options || {};
        if (type === 'attributes' && !opt.attributes) return false;
        if (type === 'childList' && !opt.childList) return false;
        if (type === 'characterData' && !opt.characterData) return false;
        if (!isSameTarget && !opt.subtree) return false;
        if (type === 'attributes' && opt.attributeFilter && Array.isArray(opt.attributeFilter) && data && data.attributeName) {
          var found = false; var name = String(data.attributeName);
          for (var i=0;i<opt.attributeFilter.length;i++){ if (String(opt.attributeFilter[i]) === name) { found = true; break; } }
          if (!found) return false;
        }
        return true;
      }
      // Internal enqueue; details: {attributeName?, oldValue?, added?, removed?} with keys arrays
      if (typeof d.__valorEnqueueMutation !== 'function') {
        d.__valorEnqueueMutation = function(type, targetKey, details) {
          try {
            var targetHandle = d.__valorMakeHandle(targetKey);
            var addedNodes = [];
            var removedNodes = [];
            if (details && Array.isArray(details.added)) { for (var i=0;i<details.added.length;i++){ addedNodes.push(d.__valorMakeHandle(details.added[i])); } }
            if (details && Array.isArray(details.removed)) { for (var i=0;i<details.removed.length;i++){ removedNodes.push(d.__valorMakeHandle(details.removed[i])); } }
            var attributeName = details && details.attributeName ? String(details.attributeName) : undefined;
            var oldValue = (details && details.oldValue != null) ? String(details.oldValue) : undefined;
            // Walk target and its ancestors to find matching registrations
            var cur = String(targetKey);
            var visited = 0;
            while (cur && visited < 2048) {
              visited++;
              var regs = __moGetRegsFor(cur);
              var same = (cur === String(targetKey));
              for (var i=0; i<regs.length; i++) {
                var reg = regs[i]; if (!reg || !reg.observer) continue;
                if (!__moMatches(reg, type, { attributeName: attributeName }, same)) continue;
                var rec = { type: type, target: targetHandle };
                if (type === 'attributes') { rec.attributeName = attributeName; if (reg.options && reg.options.attributeOldValue) { rec.oldValue = oldValue; } }
                if (type === 'characterData') { if (reg.options && reg.options.characterDataOldValue) { rec.oldValue = oldValue; } }
                if (type === 'childList') { rec.addedNodes = addedNodes.slice(); rec.removedNodes = removedNodes.slice(); rec.previousSibling = null; rec.nextSibling = null; }
                reg.observer._records.push(rec);
              }
              var parent = String(d.__valorHost_getParentKey(cur) || '');
              if (!parent) break;
              cur = parent;
            }
            __moScheduleDelivery();
          } catch(_) {}
        };
      }
      if (typeof globalThis.MutationObserver === 'undefined') {
        function MutationObserver(callback) { this._callback = (typeof callback === 'function') ? callback : function(){}; this._id = d.__moNextId++; this._records = []; this._observed = []; d.__moObservers[this._id] = this; }
        MutationObserver.prototype.observe = function(target, options){
          if (!target || !target.__nodeKey) return;
          var key = String(target.__nodeKey);
          var opt = options || {};
          var normalized = { attributes: !!opt.attributes, childList: !!opt.childList, characterData: !!opt.characterData, subtree: !!opt.subtree };
          if (opt.attributeFilter && Array.isArray(opt.attributeFilter)) { normalized.attributeFilter = opt.attributeFilter.slice().map(function(s){ return String(s); }); }
          if (opt.attributeOldValue) normalized.attributeOldValue = true;
          if (opt.characterDataOldValue) normalized.characterDataOldValue = true;
          var reg = { observer: this, targetKey: key, options: normalized };
          __moGetRegsFor(key).push(reg);
          this._observed.push(key);
        };
        MutationObserver.prototype.disconnect = function(){
          for (var i=0;i<this._observed.length;i++){
            var k = this._observed[i]; var regs = __moGetRegsFor(k);
            for (var j=regs.length-1;j>=0;j--){ if (regs[j] && regs[j].observer === this) { regs.splice(j,1); } }
          }
          this._observed = []; this._records = [];
        };
        MutationObserver.prototype.takeRecords = function(){ var out = this._records.slice(); this._records.length = 0; return out; };
        globalThis.MutationObserver = MutationObserver;
        if (!d.__valorDeliverMutationRecords) {
          d.__valorDeliverMutationRecords = function(){
            try {
              var any = 0;
              var map = d.__moObservers;
              for (var id in map) {
                var ob = map[id]; if (!ob || !ob._records || ob._records.length === 0) continue;
                var records = ob.takeRecords(); any += records.length;
                try { ob._callback(records, ob); } catch(_) {}
              }
              return any;
            } catch(_) { return 0; }
          };
        }
      }

      // =====================
      // Event model (basic)
      // =====================
      if (typeof d.__valorNodeListeners === 'undefined') {
        Object.defineProperty(d, '__valorNodeListeners', { value: Object.create(null), enumerable: false, writable: true });
      }
      function __parseListenerOptions(options) {
        var capture = false, once = false, passive = false;
        if (options === true) { capture = true; }
        else if (options && typeof options === 'object') {
          capture = !!options.capture; once = !!options.once; passive = !!options.passive;
        }
        return { capture: capture, once: once, passive: passive };
      }
      function __bucketFor(nodeKey, type) {
        var byNode = d.__valorNodeListeners;
        var node = byNode[nodeKey]; if (!node) { node = byNode[nodeKey] = Object.create(null); }
        var list = node[type]; if (!Array.isArray(list)) { list = node[type] = []; }
        return list;
      }
      function __removeListener(nodeKey, type, listener, capture) {
        var list = __bucketFor(nodeKey, type);
        for (var i = list.length - 1; i >= 0; i--) {
          var it = list[i];
          if (it && it.fn === listener && !!it.capture === !!capture) { list.splice(i, 1); }
        }
      }
      function __invokePhase(nodeKey, type, wantCapture, event, currentTarget) {
        var list = __bucketFor(nodeKey, type);
        for (var i = 0; i < list.length; i++) {
          var it = list[i]; if (!it) continue;
          if (!!it.capture !== !!wantCapture) continue;
          if (event.__immediateStopped) break;
          event.currentTarget = currentTarget;
          try { it.fn.call(currentTarget, event); } catch(_) {}
          if (it.once) { list.splice(i, 1); i--; }
        }
      }
      if (!globalThis.Event) {
        function Event(type, init) {
          this.type = String(type);
          this.bubbles = !!(init && init.bubbles);
          this.cancelable = !!(init && init.cancelable);
          this.defaultPrevented = false;
          this.target = null; this.currentTarget = null;
          this.eventPhase = 0; // 0 none, 1 capture, 2 target, 3 bubble
          this.__stopped = false; this.__immediateStopped = false;
        }
        Event.prototype.stopPropagation = function(){ this.__stopped = true; };
        Event.prototype.stopImmediatePropagation = function(){ this.__stopped = true; this.__immediateStopped = true; };
        Event.prototype.preventDefault = function(){ if (this.cancelable) this.defaultPrevented = true; };
        Event.CAPTURING_PHASE = 1; Event.AT_TARGET = 2; Event.BUBBLING_PHASE = 3;
        globalThis.Event = Event;
      }
      if (!globalThis.CustomEvent) {
        function CustomEvent(type, init) { Event.call(this, type, init); this.detail = init && init.detail; }
        CustomEvent.prototype = Object.create(Event.prototype);
        globalThis.CustomEvent = CustomEvent;
      }
      // Common event classes
      if (!globalThis.MouseEvent) {
        function MouseEvent(type, init) {
          Event.call(this, type, init);
          init = init || {};
          this.clientX = Number(init.clientX) || 0;
          this.clientY = Number(init.clientY) || 0;
          this.button = Number(init.button) || 0;
          this.buttons = Number(init.buttons) || 0;
          this.altKey = !!init.altKey;
          this.ctrlKey = !!init.ctrlKey;
          this.shiftKey = !!init.shiftKey;
          this.metaKey = !!init.metaKey;
        }
        MouseEvent.prototype = Object.create(Event.prototype);
        globalThis.MouseEvent = MouseEvent;
      }
      if (!globalThis.KeyboardEvent) {
        function KeyboardEvent(type, init) {
          Event.call(this, type, init);
          init = init || {};
          this.key = (init.key == null) ? '' : String(init.key);
          this.code = (init.code == null) ? '' : String(init.code);
          this.altKey = !!init.altKey;
          this.ctrlKey = !!init.ctrlKey;
          this.shiftKey = !!init.shiftKey;
          this.metaKey = !!init.metaKey;
          this.repeat = !!init.repeat;
        }
        KeyboardEvent.prototype = Object.create(Event.prototype);
        globalThis.KeyboardEvent = KeyboardEvent;
      }
      // Install EventTarget APIs on element handles
      if (!d.__valorEventTargetAugmented) {
        d.__valorEventTargetAugmented = true;
        var oldMake = d.__valorMakeHandle;
        d.__valorMakeHandle = function(key) {
          var o = oldMake(key);
          o.addEventListener = function(type, listener, options) {
            if (!type || typeof listener !== 'function') return;
            var opts = __parseListenerOptions(options);
            var list = __bucketFor(this.__nodeKey, String(type));
            list.push({ fn: listener, capture: !!opts.capture, once: !!opts.once, passive: !!opts.passive });
          };
          o.removeEventListener = function(type, listener, options) {
            var opts = __parseListenerOptions(options);
            __removeListener(this.__nodeKey, String(type), listener, !!opts.capture);
          };
          o.dispatchEvent = function(event) {
            if (!event || !event.type) return false;
            event.target = this; var type = String(event.type);
            // Build ancestor path from root->...->parent
            var path = [];
            var cur = this.__nodeKey; var parent;
            while (true) {
              parent = String(d.__valorHost_getParentKey(cur) || '');
              if (!parent) break;
              path.push(parent);
              if (parent === '0') break;
              cur = parent;
            }
            // CAPTURE
            event.eventPhase = Event.CAPTURING_PHASE;
            for (var i = path.length - 1; i >= 0 && !event.__stopped; i--) {
              var nk = path[i];
              __invokePhase(nk, type, true, event, d.__valorMakeHandle(nk));
            }
            if (event.__stopped) return !event.defaultPrevented;
            // TARGET
            event.eventPhase = Event.AT_TARGET;
            __invokePhase(this.__nodeKey, type, true, event, this);
            if (!event.__stopped) {
              __invokePhase(this.__nodeKey, type, false, event, this);
            }
            if (event.__stopped) return !event.defaultPrevented;
            // BUBBLE
            if (event.bubbles) {
              event.eventPhase = Event.BUBBLING_PHASE;
              for (var j = 0; j < path.length && !event.__stopped; j++) {
                var nk2 = path[j];
                __invokePhase(nk2, type, false, event, d.__valorMakeHandle(nk2));
              }
            }
            event.eventPhase = 0; event.currentTarget = null;
            return !event.defaultPrevented;
          };
          return o;
        };
      }

      // Polyfills and utilities: queueMicrotask, performance.now, Storage
      if (typeof globalThis.queueMicrotask !== 'function') {
        globalThis.queueMicrotask = function(cb){ return Promise.resolve().then(function(){ try { cb && cb(); } catch(_){} }); };
      }
      if (typeof globalThis.performance === 'undefined') { globalThis.performance = {}; }
      if (typeof globalThis.performance.now !== 'function') {
        globalThis.performance.now = function(){
          try { if (globalThis.performance && typeof globalThis.performance.now === 'function') { /* noop */ } } catch(_){}
          try { var f = (globalThis.performance && globalThis.performance.now) ? null : null; } catch(_){}
          try { if (document && typeof document.__valorHost_performanceNow === 'function') { return Number(document.__valorHost_performanceNow()); } } catch(_){}
          return Date.now();
        };
      }
      function __makeStorage(kind){
        return {
          getItem: function(key){ try { if (document.__valorHost_storage_hasItem(kind, String(key))) { return String(document.__valorHost_storage_getItem(kind, String(key))); } else { return null; } } catch(_) { return null; } },
          setItem: function(key, value){ try { document.__valorHost_storage_setItem(kind, String(key), String(value)); } catch(_){} },
          removeItem: function(key){ try { document.__valorHost_storage_removeItem(kind, String(key)); } catch(_){} },
          clear: function(){ try { document.__valorHost_storage_clear(kind); } catch(_){} },
          key: function(index){ try { var s = String(document.__valorHost_storage_keys(kind)||''); var arr = s.trim()? s.trim().split(/\s+/): []; var i = Number(index)||0; return (i>=0 && i<arr.length) ? arr[i] : null; } catch(_) { return null; } },
          get length(){ try { var s = String(document.__valorHost_storage_keys(kind)||''); if (!s.trim()) return 0; return s.trim().split(/\s+/).length; } catch(_) { return 0; } }
        };
      }
      if (typeof globalThis.localStorage === 'undefined') { try { globalThis.localStorage = __makeStorage('local'); } catch(_){} }
      if (typeof globalThis.sessionStorage === 'undefined') { try { globalThis.sessionStorage = __makeStorage('session'); } catch(_){} }

      // =====================
      // Timers implementation
      // =====================
      if (typeof globalThis.__valorTimers === 'undefined') {
        Object.defineProperty(globalThis, '__valorTimers', { value: {
          nextId: 1,
          entries: Object.create(null), // id -> { id, callback, delayMs, nextFireMs, interval, args, cancelled }
          queue: [] // array of ids
        }, enumerable: false, writable: true });
      }

      function coerceDelayMs(value) {
        var n = Number(value);
        if (!isFinite(n) || n < 0) n = 0;
        // Per HTML spec, clamp to 1ms as a floor for nested timers; we keep it simple here
        return Math.floor(n);
      }

      function scheduleTimer(callback, delay, isInterval, args) {
        if (typeof callback !== 'function') { return 0; }
        var timers = globalThis.__valorTimers;
        var id = timers.nextId++;
        var delayMs = coerceDelayMs(delay);
        var now = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
        var entry = { id: id, callback: callback, delayMs: delayMs, nextFireMs: now + delayMs, interval: !!isInterval, args: Array.prototype.slice.call(args || []), cancelled: false };
        timers.entries[id] = entry;
        timers.queue.push(id);
        return id;
      }

      if (typeof globalThis.setTimeout !== 'function') {
        globalThis.setTimeout = function(cb, delay) { return scheduleTimer(cb, delay, false, Array.prototype.slice.call(arguments, 2)); };
      }
      if (typeof globalThis.setInterval !== 'function') {
        globalThis.setInterval = function(cb, delay) { return scheduleTimer(cb, delay, true, Array.prototype.slice.call(arguments, 2)); };
      }
      if (typeof globalThis.clearTimeout !== 'function') {
        globalThis.clearTimeout = function(id) { var t = globalThis.__valorTimers; var e = t && t.entries[id]; if (e) { e.cancelled = true; delete t.entries[id]; } };
      }
      if (typeof globalThis.clearInterval !== 'function') {
        globalThis.clearInterval = function(id) { var t = globalThis.__valorTimers; var e = t && t.entries[id]; if (e) { e.cancelled = true; delete t.entries[id]; } };
      }

      // Host-driven tick: executes at most one due timer callback per call for predictable ordering.
      if (typeof globalThis.__valorTickTimersOnce !== 'function') {
        globalThis.__valorTickTimersOnce = function(nowMs) {
          var timers = globalThis.__valorTimers;
          if (!timers) return 0;
          var now = Number(nowMs);
          if (!isFinite(now)) { now = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now(); }
          var chosenIndex = -1;
          var chosenId = 0;
          var chosenEntry = null;
          var minFire = Infinity;
          // Find the earliest due timer in insertion order
          for (var i = 0; i < timers.queue.length; i++) {
            var id = timers.queue[i];
            var e = timers.entries[id];
            if (!e || e.cancelled) { continue; }
            if (e.nextFireMs <= now && e.nextFireMs < minFire) {
              minFire = e.nextFireMs;
              chosenIndex = i;
              chosenId = id;
              chosenEntry = e;
            }
          }
          if (!chosenEntry) { return 0; }
          // Remove from queue position (keep others intact)
          timers.queue.splice(chosenIndex, 1);
          try {
            chosenEntry.callback.apply(undefined, chosenEntry.args);
          } catch (_) {
            // Swallow for now; error reporting is future work
          }
          if (chosenEntry.interval && !chosenEntry.cancelled) {
            // Reschedule interval from now
            chosenEntry.nextFireMs = now + Math.max(1, chosenEntry.delayMs);
            timers.queue.push(chosenId);
            timers.entries[chosenId] = chosenEntry;
          } else {
            // One-shot: clear it
            delete timers.entries[chosenId];
          }
          return 1;
        };
      }

      // =====================
      // Networking polyfills: fetch and XMLHttpRequest
      // =====================
      (function(){
        var doc = globalThis.document || {};
        // Helpers
        function __pollRequest(id, cb){
          function step(){
            try {
              var json = String(doc.__valorHost_net_requestPoll(id));
              var s = JSON.parse(json);
              if (s.state === 'pending') {
                // Use a macrotask to yield back to the host between polls.
                // Using microtasks here can lead to an infinite drain within a single host tick
                // because the engine flushes microtasks until empty.
                setTimeout(step, 0);
              } else {
                cb(s);
              }
            } catch (_) {
              cb({ state: 'error', error: 'poll-failed' });
            }
          }
          step();
        }

        if (typeof globalThis.Headers === 'undefined') {
          function Headers(init){ this._map = {}; if (init && typeof init === 'object') { for (var k in init) { if (Object.prototype.hasOwnProperty.call(init, k)) { this._map[String(k).toLowerCase()] = String(init[k]); } } } }
          Headers.prototype.get = function(name){ return this._map[String(name).toLowerCase()] || null; };
          Headers.prototype.set = function(name, value){ this._map[String(name).toLowerCase()] = String(value); };
          Headers.prototype.append = function(name, value){ var k = String(name).toLowerCase(); var prev = this._map[k]; this._map[k] = prev ? (prev + ', ' + String(value)) : String(value); };
          Headers.prototype.has = function(name){ return Object.prototype.hasOwnProperty.call(this._map, String(name).toLowerCase()); };
          Headers.prototype.toJSON = function(){ return this._map; };
          globalThis.Headers = Headers;
        }
        if (typeof globalThis.Request === 'undefined') {
          function Request(input, init){ init = init || {}; this.method = (init.method || 'GET').toString().toUpperCase(); this.url = String(input || ''); this.headers = new Headers(init.headers || {}); this.body = init.body || null; }
          globalThis.Request = Request;
        }
        if (typeof globalThis.Response === 'undefined') {
          function Response(bodyText, init){ init = init || {}; this.status = Number(init.status)||200; this.statusText = String(init.statusText||''); this.ok = !!init.ok; this.headers = new Headers(init.headers||{}); this._bodyText = String(bodyText||''); this._bodyBase64 = String(init.bodyBase64||''); }
          Response.prototype.text = function(){ return Promise.resolve(this._bodyText); };
          Response.prototype.json = function(){ try { return Promise.resolve(JSON.parse(this._bodyText)); } catch (e) { return Promise.reject(e); } };
          Response.prototype.arrayBuffer = function(){
            // Base64 -> Uint8Array minimal decoder
            var alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
            var s = this._bodyBase64 || '';
            var out = [];
            for (var i=0;i<s.length;i+=4){ var c1 = alphabet.indexOf(s[i]); var c2 = alphabet.indexOf(s[i+1]); var c3 = alphabet.indexOf(s[i+2]); var c4 = alphabet.indexOf(s[i+3]); var n = (c1<<18)|(c2<<12)|((c3&63)<<6)|(c4&63); out.push((n>>16)&255); if (s[i+2] !== '=') out.push((n>>8)&255); if (s[i+3] !== '=') out.push(n&255); }
            return Promise.resolve(new Uint8Array(out).buffer);
          };
          globalThis.Response = Response;
        }
        if (typeof globalThis.fetch !== 'function') {
          globalThis.fetch = function(input, init){
            try {
              var req = (input && typeof input === 'object' && input.url) ? input : new Request(input, init);
              var headersJson = JSON.stringify(req.headers && req.headers.toJSON ? req.headers.toJSON() : {});
              var bodyBase64 = (typeof req.body === 'string') ? btoa(unescape(encodeURIComponent(req.body))) : '';
              var id = String(doc.__valorHost_net_requestStart(req.method, req.url, headersJson, bodyBase64));
              return new Promise(function(resolve, reject){
                __pollRequest(id, function(res){
                  if (res.state === 'done' && !res.error) {
                    var resp = new Response(res.bodyText || '', { status: res.status||0, statusText: res.statusText||'', ok: !!res.ok, headers: (function(){ var m={}; for (var i=0;i<(res.headers||[]).length;i++){ var it = res.headers[i]; m[it[0]] = it[1]; } return m; })(), bodyBase64: res.bodyBase64||'' });
                    resolve(resp);
                  } else {
                    reject(new TypeError(res.error || 'Network error'));
                  }
                });
              });
            } catch (e) {
              return Promise.reject(e);
            }
          };
        }
        if (typeof globalThis.XMLHttpRequest === 'undefined') {
          function XMLHttpRequest(){ this.readyState = 0; this.status = 0; this.statusText = ''; this.responseText=''; this.onreadystatechange=null; this._headers = {}; this._method='GET'; this._url=''; }
          XMLHttpRequest.prototype.open = function(method, url){ this._method = String(method||'GET').toUpperCase(); this._url = String(url||''); this.readyState = 1; if (typeof this.onreadystatechange==='function') try{ this.onreadystatechange(); }catch(_){ } };
          XMLHttpRequest.prototype.setRequestHeader = function(name, value){ this._headers[String(name)] = String(value); };
          XMLHttpRequest.prototype.send = function(body){ var self = this; var headersJson = JSON.stringify(this._headers||{}); var bodyB64 = (typeof body === 'string') ? btoa(unescape(encodeURIComponent(body))) : ''; var id = String(doc.__valorHost_net_requestStart(this._method, this._url, headersJson, bodyB64)); __pollRequest(id, function(res){ if (res.state==='done'){ self.status = Number(res.status)||0; self.statusText = String(res.statusText||''); self.responseText = String(res.bodyText||''); self.readyState = 4; if (typeof self.onreadystatechange==='function') try{ self.onreadystatechange(); }catch(_){ } } else { self.status = 0; self.statusText=''; self.responseText=''; self.readyState = 4; if (typeof self.onreadystatechange==='function') try{ self.onreadystatechange(); }catch(_){ } } }); };
          globalThis.XMLHttpRequest = XMLHttpRequest;
        }
      })();
    })();
    ";
