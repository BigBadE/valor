use crate::state::HtmlPage;
use anyhow::Error;

pub trait JsRuntime {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    fn tick_timers_once(&mut self, page: &mut HtmlPage);
    fn drive_after_dom_update<'a>(
        &'a mut self,
        page: &'a mut HtmlPage,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<(), Error>> + 'a>>;
}

#[derive(Default)]
pub struct DefaultJsRuntime;

impl JsRuntime for DefaultJsRuntime {
    fn name(&self) -> &'static str {
        "default"
    }
    fn tick_timers_once(&mut self, page: &mut HtmlPage) {
        page.tick_js_timers_once();
    }
    fn drive_after_dom_update<'a>(
        &'a mut self,
        page: &'a mut HtmlPage,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<(), Error>> + 'a>> {
        Box::pin(async move {
            page.execute_pending_scripts();
            page.handle_dom_content_loaded_if_needed().await
        })
    }
}
