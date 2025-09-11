//! Criterion benchmarks comparing full reflow vs incremental reflow.
//!
//! These benches build a synthetic DOM-like tree directly via DOMUpdate batches
//! applied to the Layouter mirror, then measure:
//! - Full reflow over the entire tree.
//! - Incremental reflow after a small set of attribute changes.

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use layouter::Layouter;
use js::{DOMUpdate, NodeKey};

/// Build a flat tree: ROOT -> body -> N divs (block), each with one text child.
fn build_flat_tree(l: &mut Layouter, n: usize) -> (NodeKey, Vec<NodeKey>, Vec<NodeKey>) {
    let body_key = NodeKey::pack(1, 1, 1);
    // Insert <body> under ROOT
    let _ = l.apply_update(DOMUpdate::InsertElement { parent: NodeKey::ROOT, node: body_key, tag: "body".into(), pos: 0 });
    let mut element_keys = Vec::with_capacity(n);
    let mut text_keys = Vec::with_capacity(n);
    for i in 0..n {
        let elem_key = NodeKey::pack(1, 1, (i as u64) + 2);
        let text_key = NodeKey::pack(1, 1, (i as u64) + 2 + n as u64);
        let _ = l.apply_update(DOMUpdate::InsertElement { parent: body_key, node: elem_key, tag: "div".into(), pos: i });
        let _ = l.apply_update(DOMUpdate::InsertText { parent: elem_key, node: text_key, text: format!("hello {} world", i), pos: 0 });
        element_keys.push(elem_key);
        text_keys.push(text_key);
    }
    // Do an initial full compute to seed caches
    let _ = l.compute_layout_full_for_bench();
    (body_key, element_keys, text_keys)
}

/// Apply style-like mutations by setting a class attribute on a subset of nodes.
fn apply_small_style_mutations(l: &mut Layouter, nodes: &[NodeKey], percent: f32) {
    let count = ((nodes.len() as f32) * percent).max(1.0) as usize;
    for (i, key) in nodes.iter().take(count).enumerate() {
        let _ = l.apply_update(DOMUpdate::SetAttr { node: *key, name: "class".into(), value: format!("toggled-{}", i) });
    }
}

fn bench_reflow(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_reflow");
    for &n in &[500usize, 2_000usize, 5_000usize] {
        // Build a fresh layouter per size
        let mut layouter = Layouter::new();
        let (_body, element_keys, _text_keys) = build_flat_tree(&mut layouter, n);

        // Full reflow benchmark
        group.bench_with_input(BenchmarkId::new("full_reflow", n), &n, |b, &_n| {
            b.iter(|| {
                // Force full compute; black_box to prevent elimination
                let processed = layouter.compute_layout_full_for_bench();
                black_box(processed);
            })
        });

        // Incremental reflow benchmark (1% of elements mutated by attribute)
        group.bench_with_input(BenchmarkId::new("partial_reflow_1pct_attr", n), &n, |b, &_n| {
            b.iter(|| {
                apply_small_style_mutations(&mut layouter, &element_keys, 0.01);
                let processed = layouter.compute_layout_incremental_for_bench();
                black_box(processed);
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_reflow);
criterion_main!(benches);
