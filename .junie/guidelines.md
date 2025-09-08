General tips:
- Don't use full import paths
- Import at the top of the file instead of in functions, the only exception is matching on an enum
- Use full variable names, not shorthands like k, v, e, vv, etc...
- Prefer using functional-style functions over for/while loops and ifs if possible and clean
- Reduce nesting when possible, invert ifs and break apart functions to prevent too complex of functions
- Try to keep functions under 50-100 lines if possible
- Make sure not to repeat over 3 of the same lines of code

Design: Browser Data Flow in Valor

Overview
- Valor is organized into crates: html (DOM, parser), css (parser and types), layouter (layout tree mirror), wgpu_renderer (render backend), page_handler (page orchestration), and valor (app entry and event loop).
- Data flows as a stream of DOMUpdate events through a publish/subscribe channel, mirrored by components like the Layouter. CSS stylesheets flow alongside HTML, parsed into a Stylesheet model and used by layout/style calculation.

Key Types and Actors
- page_handler::state::HtmlPage: Orchestrates loading of a page, sets up channels, owns the DOM, and runs the HTML parser.
- html::dom::DOM: Runtime DOM that applies incoming updates and broadcasts them to subscribers.
- html::dom::updating::{DOMUpdate, DOMMirror<T>, DOMSubscriber}: Event model and mirror pattern for propagating DOM mutations to interested components.
- html::parser::HTMLParser: Drives streaming HTML parsing (via html5ever) and emits DOMUpdate batches.
- html::parser::html5ever_engine::Html5everEngine and ValorSink: Bridge html5ever to ParserDOMMirror to create elements/text/attrs and append/remove.
- layouter::Layouter: Implements DOMSubscriber; maintains a layout tree mirror of the DOM, later used for layout and rendering.
- css::parser::StylesheetStreamParser and css::types::{Stylesheet, StyleRule, Declaration, Origin}: Parse and represent CSS for style resolution.
- wgpu_renderer::state::RenderState: GPU rendering state that will consume layout/display lists for drawing.

Channels and Mirroring
- html::dom::DOM is constructed with two channels:
  - out_updater: broadcast::Sender<Vec<DOMUpdate>> — DOM broadcasts updates it applied so mirrors can reflect them.
  - in_receiver: mpsc::Receiver<Vec<DOMUpdate>> — DOM receives updates from producers (e.g., parsers or interactive scripts).
- DOMMirror<T> wraps a mirror implementation T: DOMSubscriber with:
  - in_updater: broadcast::Receiver<Vec<DOMUpdate>> — to receive DOMUpdate batches from the DOM.
  - out_updater: mpsc::Sender<Vec<DOMUpdate>> — to send changes back to the DOM if needed (e.g., mirror-originated edits).

End-to-End Flow (HTML + CSS + Layout + Render)
1) App startup
   - crates/valor/src/main.rs creates winit window and a tokio Runtime.
   - It constructs page_handler::state::HtmlPage via HtmlPage::new(handle, url).

2) HtmlPage initialization
   - Creates two channels: (out_updater, out_receiver) broadcast; (in_updater, in_receiver) mpsc.
   - DOM::new(out_updater, in_receiver) is created and stored.
   - DOM registers a NodeKeyManager shard for the parser to obtain stable NodeKeys.
   - HTMLParser::parse(handle, in_updater.clone(), keyman, stream_url(url), out_receiver) returns a loader that streams bytes and produces DOMUpdate batches.

3) HTML parsing and DOM updates
   - HTMLParser drives html5ever via Html5everEngine/ValorSink.
   - ValorSink methods (create_element, append, set attributes, etc.) call into ParserDOMMirror to enqueue fine-grained DOMUpdate values:
     - InsertElement { parent, node, tag, pos }
     - InsertText { parent, node, text, pos }
     - SetAttr { node, name, value }
     - RemoveNode { node }
     - EndOfDocument
   - ParserDOMMirror batches updates and sends them to the DOM through the in_updater channel.
   - DOM.update() drains in_receiver, applies each DOMUpdate to its internal tree, and then re-broadcasts the batch to out_updater for all subscribers.

4) CSS discovery and parsing (design intent)
   - While parsing HTML, when encountering:
     - &lt;link rel="stylesheet" href="..."&gt;: schedule a subresource fetch for the CSS and stream it; feed chunks into css::parser::StylesheetStreamParser.
     - &lt;style&gt; ... &lt;/style&gt;: feed the text content directly into StylesheetStreamParser.
   - StylesheetStreamParser incrementally parses chunks into css::types::Stylesheet (rules: selectors + declarations) with source ordering and origin.
   - The resulting Stylesheet(s) are sent to style consumers (e.g., Layouter or a StyleEngine) via one of:
     - A side-channel (future: dedicated CSSUpdate broadcast), or
     - Embedding stylesheet availability on the DOM (e.g., attaching to <style>/<link> nodes) and notifying mirrors via DOMUpdate.

5) Mirroring in Layouter
   - The Layouter implements DOMSubscriber and maintains its own layout tree (a simplified block/text model right now):
     - InsertElement → ensure a Block layout node exists; attach by position.
     - InsertText → ensure an InlineText node; attach by position.
     - SetAttr → update attributes on the layout node mirror.
     - RemoveNode → detach and recursively remove mirrored nodes.
     - EndOfDocument → currently a no-op; can trigger layout finalize.
   - LayouterMirror = DOMMirror<Layouter> connects the Layouter to the DOM’s out_updater and allows sending edits back if needed.
   - With CSS available, Layouter (or a future StyleEngine) will:
     - Match CSS selectors against the DOM (using css::selector types) to compute cascaded styles.
     - Produce computed styles per element, then perform a layout pass to compute geometry.
     - Generate a display list for the renderer.

6) Rendering
   - RenderState (wgpu_renderer) will consume the computed layout/display list and draw each frame.
   - The main loop calls HtmlPage::update() to keep applying DOM updates and then RenderState::render() to draw.

Update Scheduling and Lifecycle
- HtmlPage::update():
  - If the HTMLParser signals is_finished(), finalize it (await the task), then ensure DOM.update() drains remaining changes.
  - Call DOM.update() each tick to propagate any pending updates to mirrors.
- DOM.update() behavior:
  - Applies each DOMUpdate to its internal tree.
  - Broadcasts the batch to all subscribers; mirrors process updates via DOMMirror<T>::update/try_update_sync.
- EndOfDocument:
  - HTML parser emits DOMUpdate::EndOfDocument once input is exhausted, allowing mirrors to run finalize steps (e.g., style resolve/layout finalize).

Example: Wiring a Layouter Mirror to a Page
- Create a Layouter and attach it to the page’s DOM stream.

  let mut page: HtmlPage = /* created by HtmlPage::new(...) */;
  let layouter = layouter::Layouter::new();
  let mut layouter_mirror = page.create_mirror(layouter);

  // In a loop or update tick (blocking thread variant):
  layouter_mirror.try_update_sync()?; // Drain pending DOMUpdate batches
  let node_count = layouter_mirror.mirror_mut().compute_layout();

Data Ownership and Keys
- NodeKey and NodeKeyManager provide stable 64-bit keys assigned per producer shard so that DOM and mirrors can correlate nodes across asynchronous updates.
- DOM maintains a mapping from NodeKey → runtime NodeId (indextree), updating or reusing nodes as updates arrive.

Future Integration Notes
- CSS to Layout wiring: Introduce a style engine that subscribes to DOM updates, maintains a Stylesheet set, performs selector matching, computes cascaded styles, and passes computed styles to Layouter before layout passes.
- Subresource loading: page_handler::url::stream_url is used for HTML; reuse for stylesheets; maintain source order for cascade.
- Scripting/events: Additional producers (e.g., JS engine) can send DOMUpdate batches through the DOM’s in_updater mpsc channel.

Summary
- Page creates HTMLParser and DOM; parser emits DOMUpdate; DOM applies and broadcasts; Layouter mirrors DOM and (with CSS) computes layout; renderer draws. Stylesheets flow from HTML discovery through css::parser into style resolution and layout.
