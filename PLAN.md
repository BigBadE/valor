# CSS Pipeline Design

A lazy, pull-based system that minimizes work by only computing properties for interested nodes.

## Core Principles

1. **Sparse storage** - Database only stores properties for interested nodes
2. **Pull-based** - Values computed on demand, not eagerly
3. **Fine-grained invalidation** - Per-property, per-axis; height changes don't cause width recomputation
4. **Confidence-gated updates** - Low specificity rules wait until head parsing completes
5. **Dependency collapsing** - Formulas like `child.width = parent.width` apply diffs directly

## Components

### StreamingCssParser
Parses CSS in a separate task, emits rules via callback as they complete. Uses lightningcss for full CSS spec compliance with typed `Property<'static>` values.

### RuleStorage
Stores complex selectors only. Simple selectors (single id/class) are O(1) lookups at match time - no need to store them.

### Database
Sparse storage of properties for interested nodes. When interest drops (node goes offscreen), properties are dropped. They can be recomputed from matched rules when needed.

### Subscriptions
Broadcasts property changes to subscribers with confidence level (based on specificity). Subscribers decide whether to act immediately or wait.

### Formula
Static description of how to compute a layout value. The formula itself encodes dependencies - traversing it reveals what inputs it needs. Used for:
- Collapsing simple dependencies (`child.width = parent.width` → direct copy)
- Batch evaluation of complex computations

### Query
Selects which Formula to use based on node CSS properties. Returns a `&'static Formula` pointer. When the pointer changes (e.g., display switches from block to flex), recomputation is triggered.

## Data Flow

```
CSS Text
    │
    ▼
StreamingCssParser (separate task)
    │
    ▼ emits ParsedRule { selectors, properties }
    │
    ▼
Rule Matching
    ├── Simple selectors: O(1) lookup (check node's id/classes directly)
    └── Complex selectors: check against RuleStorage
    │
    ▼
Subscription Callback (with confidence = specificity)
    │
    ├── High confidence (ID selector, etc): callback immediately
    └── Low confidence: wait until head parsing completes
    │
    ▼
Renderer Interest Check
    ├── Is parent visible?
    │   ├── Yes: interested in most properties
    │   └── No: only interested if property could shift node on-screen
    └── Filter internally based on node
    │
    ▼
Database Update (for interested properties only)
    │
    ▼
Query Evaluation
    ├── Same Formula pointer? → check if input values changed
    └── Different Formula pointer? → recompute with new formula
    │
    ▼
Formula Evaluation
    ├── Simple (direct reference): copy value directly
    └── Complex: evaluate formula, propagate through dependency graph
    │
    ▼
Diff Propagation (if value changed)
```

## Confidence System

Based on CSS specificity:
- **High confidence**: ID selectors, inline styles - unlikely to be overridden, act immediately
- **Low confidence**: Element selectors, low-specificity class selectors - wait for head to finish
- **After head completes**: Accept everything regardless of confidence

## Interest Model

The renderer drives interest:
1. Visible nodes: interested in all layout/visual properties
2. Offscreen nodes: only interested if property could bring them on-screen (e.g., position, transform)
3. When interest drops: properties removed from Database
4. When interest returns: recompute from matched rules in RuleStorage

## Formula Dependencies

Formulas describe computation, dependencies are implicit:
```
child.width = parent.width
```
Traversing this formula shows it depends on `parent.width`. When `parent.width` changes:
1. Find formulas referencing it
2. If formula is direct reference → copy new value
3. If formula is complex → evaluate with new input

No separate dependency tracking needed - the Formula *is* the dependency graph.

## Optimizations

### Implemented
- Sparse property storage
- Confidence-gated callbacks
- Formula-based dependency collapsing
- Per-property, per-axis invalidation

### Future Considerations
- Bloom filter for ancestor classes/ids to quickly reject selector non-matches
- Rule grouping by rightmost selector
- Property batching for shorthands
- SIMD/batch formula evaluation for similar nodes

## Comparison to Chromium

**We win on:**
- Offscreen content: we don't compute it at all
- Incremental updates: fine-grained vs. "restyle subtree"
- Memory: sparse storage vs. full computed style on every node

**Chromium wins on:**
- Initial full page render: highly optimized C++
- Complex selectors: decades of optimization

**Target advantage:** Interactive performance on complex pages with frequent updates.

## Open Questions

1. **Late stylesheets** - Stylesheets in body or injected via JS. Policy for when to stop waiting?
2. **Inherited properties** - Efficient propagation without full subtree traversal?
3. **Dynamic pseudo-classes** - `:hover`, `:focus` change without new CSS. Track which rules have them?
4. **`!important`** - Breaks normal specificity. Handle in confidence system?
