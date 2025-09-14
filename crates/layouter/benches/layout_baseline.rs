use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use js::{DOMUpdate, NodeKey};
use layouter::Layouter;
use std::collections::HashMap;
use style_engine::ComputedStyle;

/// Build a small synthetic DOM in the Layouter mirror for benchmarking.
fn build_small_dom() -> Layouter {
    let mut l = Layouter::new();
    let root = NodeKey::ROOT;
    // <html><body>...
    let html = NodeKey(10);
    let body = NodeKey(11);
    l.apply_update(DOMUpdate::InsertElement { parent: root, node: html, tag: "html".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: html, node: body, tag: "body".into(), pos: 0 }).unwrap();

    // <div><p>Some text</p><p>More text…</p></div>
    let div = NodeKey(12);
    let p1 = NodeKey(13);
    let t1 = NodeKey(14);
    let p2 = NodeKey(15);
    let t2 = NodeKey(16);
    l.apply_update(DOMUpdate::InsertElement { parent: body, node: div, tag: "div".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: div, node: p1, tag: "p".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: p1, node: t1, text: "Some text".into(), pos: 0 }).unwrap();
    l.apply_update(DOMUpdate::InsertElement { parent: div, node: p2, tag: "p".into(), pos: 1 }).unwrap();
    l.apply_update(DOMUpdate::InsertText { parent: p2, node: t2, text: "More text in the second paragraph".into(), pos: 0 }).unwrap();

    // Minimal computed styles for html/body/div/p/text
    let mut comp: HashMap<NodeKey, ComputedStyle> = HashMap::new();
    comp.insert(html, ComputedStyle::default());
    comp.insert(body, ComputedStyle::default());
    comp.insert(div, ComputedStyle::default());
    comp.insert(p1, ComputedStyle::default());
    comp.insert(p2, ComputedStyle::default());
    comp.insert(t1, ComputedStyle::default());
    comp.insert(t2, ComputedStyle::default());
    l.set_computed_styles(comp);

    l
}

fn bench_layout_small_dom(c: &mut Criterion) {
    // Record a baseline by measuring compute_layout + compute_layout_geometry end-to-end
    c.bench_function("layouter_small_dom_compute", |b| {
        b.iter(|| {
            let mut l = build_small_dom();
            let _count = l.compute_layout();
            let rects = l.compute_layout_geometry();
            black_box((_count, rects.len()));
        })
    });
}

criterion_group!(layout_benches, bench_layout_small_dom);
criterion_main!(layout_benches);
