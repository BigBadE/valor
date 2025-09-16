use crate::state::HtmlPage;

/// Keyboard modifier flags for key events.
#[derive(Copy, Clone, Debug, Default)]
pub struct KeyMods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}
use anyhow::Error;
use js::JsEngine;

impl HtmlPage {
    /// Dispatch a synthetic host event to the document using the JS bridge.
    /// Props must be a JSON object string (e.g., {"bubbles":true,"clientX":10}).
    pub fn host_dispatch_document(&mut self, ty: &str, props_json: &str) {
        let mut js = String::from("(function(){try{return document.__valorHostDispatch(\"");
        js.push_str(ty);
        js.push_str("\",null,");
        js.push_str(props_json);
        js.push_str(");}catch(e){return false;}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "host://dispatch_document.js");
    }

    /// Dispatch a synthetic host event to a specific target key (string form) via the JS bridge.
    /// The target_key should match the DOM handle key string used internally.
    pub fn host_dispatch_to_key(&mut self, ty: &str, target_key: &str, props_json: &str) {
        let mut js = String::from("(function(){try{return document.__valorHostDispatch(\"");
        js.push_str(ty);
        js.push_str("\",\"");
        js.push_str(&target_key.replace('"', "\\\""));
        js.push_str("\",");
        js.push_str(props_json);
        js.push_str(");}catch(e){return false;}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "host://dispatch_target.js");
    }
}

impl HtmlPage {
    /// Dispatch a synthetic pointer move event to the document.
    /// The event is delivered via the JS runtime by calling document.dispatchEvent
    /// with a plain object carrying standard MouseEvent-like fields.
    pub fn dispatch_pointer_move(&mut self, x: f64, y: f64) {
        let mut js = String::from("(function(){try{var e={type:'mousemove',clientX:");
        js.push_str(&x.to_string());
        js.push_str(",clientY:");
        js.push_str(&y.to_string());
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "valor://event/pointer_move");
        let _ = self.js_engine_mut().run_jobs();
    }

    /// Dispatch a synthetic pointer down (mouse down) event.
    pub fn dispatch_pointer_down(&mut self, x: f64, y: f64, button: u32) {
        let mut js = String::from("(function(){try{var e={type:'mousedown',clientX:");
        js.push_str(&x.to_string());
        js.push_str(",clientY:");
        js.push_str(&y.to_string());
        js.push_str(",button:");
        js.push_str(&button.to_string());
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "valor://event/pointer_down");
        let _ = self.js_engine_mut().run_jobs();
    }

    /// Dispatch a synthetic pointer up (mouse up) event.
    pub fn dispatch_pointer_up(&mut self, x: f64, y: f64, button: u32) {
        let mut js = String::from("(function(){try{var e={type:'mouseup',clientX:");
        js.push_str(&x.to_string());
        js.push_str(",clientY:");
        js.push_str(&y.to_string());
        js.push_str(",button:");
        js.push_str(&button.to_string());
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "valor://event/pointer_up");
        let _ = self.js_engine_mut().run_jobs();
    }

    /// Dispatch a synthetic keydown event with optional modifier flags.
    pub fn dispatch_key_down(&mut self, key: &str, code: &str, mods: KeyMods) {
        let mut js = String::from("(function(){try{var e={type:'keydown',key:");
        js.push_str(&format!("{:?}", key));
        js.push_str(",code:");
        js.push_str(&format!("{:?}", code));
        js.push_str(",ctrlKey:");
        js.push_str(if mods.ctrl { "true" } else { "false" });
        js.push_str(",altKey:");
        js.push_str(if mods.alt { "true" } else { "false" });
        js.push_str(",shiftKey:");
        js.push_str(if mods.shift { "true" } else { "false" });
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "valor://event/key_down");
        let _ = self.js_engine_mut().run_jobs();
    }

    /// Dispatch a synthetic keyup event with optional modifier flags.
    pub fn dispatch_key_up(&mut self, key: &str, code: &str, mods: KeyMods) {
        let mut js = String::from("(function(){try{var e={type:'keyup',key:");
        js.push_str(&format!("{:?}", key));
        js.push_str(",code:");
        js.push_str(&format!("{:?}", code));
        js.push_str(",ctrlKey:");
        js.push_str(if mods.ctrl { "true" } else { "false" });
        js.push_str(",altKey:");
        js.push_str(if mods.alt { "true" } else { "false" });
        js.push_str(",shiftKey:");
        js.push_str(if mods.shift { "true" } else { "false" });
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "valor://event/key_up");
        let _ = self.js_engine_mut().run_jobs();
    }

    /// Dispatch a synthetic text input (character) event. This is sent on ReceivedCharacter.
    pub fn dispatch_text_input(&mut self, text: &str) {
        let mut js = String::from("(function(){try{var e={type:'textinput',data:");
        js.push_str(&format!("{:?}", text));
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self
            .js_engine_mut()
            .eval_script(&js, "valor://event/text_input");
        let _ = self.js_engine_mut().run_jobs();
    }
}

impl HtmlPage {
    /// Attach a privileged chromeHost command channel to this page (for valor://chrome only).
    /// This installs the `chromeHost` namespace into the JS context with origin gating.
    pub fn attach_chrome_host(
        &mut self,
        sender: tokio::sync::mpsc::UnboundedSender<js::ChromeHostCommand>,
    ) -> Result<(), Error> {
        self.host_context_mut().chrome_host_tx = Some(sender);
        // Install the chromeHost namespace now that a channel is available
        let bindings = js::build_chrome_host_bindings();
        let host_ctx = self.host_context_mut().clone();
        let _ = self.js_engine_mut().install_bindings(&host_ctx, &bindings);
        let _ = self.js_engine_mut().run_jobs();
        Ok(())
    }
}
