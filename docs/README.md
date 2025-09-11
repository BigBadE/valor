# Valor Documentation (Phase 0)

Welcome to the Valor engine docs. This lightweight docs site provides a high‑level overview of the architecture and links to the production roadmap.

Contents
- Architecture overview (data flow and key actors)
- Development roadmap and phased checklist
- Running the layout micro‑benchmark

Architecture
See architecture.md for a concise overview of the end‑to‑end data flow: HTML/CSS parsing → DOM updates → mirrors (Layouter, StyleEngine) → layout → rendering.

Roadmap
The complete roadmap with phase checklists lives at the repository root as DESIGN_PLAN.md.

Running the layout micro‑benchmark
The layouter crate contains a Criterion benchmark to establish a baseline for compute_layout and geometry generation.

Steps:
1. Ensure you have Rust and Cargo installed.
2. From the repository root, run:
   cargo bench -p layouter
3. The benchmark will build and run; note the timing for "layouter_small_dom_compute".

Notes
- Benchmarks are for local guidance and will vary by machine.
- Phase 0 focuses on stabilization and baseline metrics; future phases will grow coverage and complexity.
