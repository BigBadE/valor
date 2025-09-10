use css::parser::StylesheetStreamParser;
use css::types::{Origin, Stylesheet};
use js::{NodeKey, DOMUpdate, DOMSubscriber};
use style_engine::{Display, StyleEngine};
use std::collections::HashMap;

/// Deterministic pseudorandom number generator for tests (xorshift64* variant).
/// This avoids adding external dependencies (like rand) and ensures reproducibility across runs.
#[derive(Clone)]
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    /// Create a new RNG with the given seed.
    pub fn new(seed: u64) -> Self {
        // Avoid a zero state which would get stuck.
        let init = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state: init }
    }

    /// Generate the next u64 value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Generate a value in [0, upper). Upper must be > 0.
    pub fn next_in_range(&mut self, upper: u64) -> u64 {
        let val = self.next_u64();
        val % upper
    }

    /// Flip a coin with probability 1/2.
    pub fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

/// Build a Stylesheet by feeding CSS text into the streaming parser to match existing tests.
fn build_stylesheet(css_text: &str) -> Stylesheet {
    let mut out = Stylesheet::default();
    let mut parser = StylesheetStreamParser::new(Origin::Author, 0);
    parser.push_chunk(css_text, &mut out);
    let extra = parser.finish();
    out.rules.extend(extra.rules);
    out
}

/// Generate a simple, mostly-supported stylesheet from the RNG.
/// Selectors include tags, classes, ids, attribute equality, and :first-child.
fn generate_random_stylesheet_text(rng: &mut DeterministicRng, rule_count: usize) -> String {
    let tags = ["div", "span", "p"]; // keep to known tags used in tests
    let classes = ["a", "b", "c", "red", "blue"]; // match other tests
    let ids = ["x", "y", "z"]; // id selectors
    let data_values = ["hero", "card", "note"]; // attribute values

    let color_values = [
        (255u8, 0u8, 0u8),
        (0, 255, 0),
        (0, 0, 255),
        (10, 20, 30),
        (200, 100, 50),
    ];

    let mut rules: Vec<String> = Vec::with_capacity(rule_count);
    for _ in 0..rule_count {
        // Choose selector kind
        let kind = rng.next_in_range(6);
        let selector = match kind {
            0 => tags[rng.next_in_range(tags.len() as u64) as usize].to_string(),
            1 => format!(".{}", classes[rng.next_in_range(classes.len() as u64) as usize]),
            2 => format!("#{}", ids[rng.next_in_range(ids.len() as u64) as usize]),
            3 => format!("[data-kind=\"{}\"]", data_values[rng.next_in_range(data_values.len() as u64) as usize]),
            4 => format!("{}:first-child", tags[rng.next_in_range(tags.len() as u64) as usize]),
            _ => {
                // Unsupported combinator on purpose should not panic; keep simple
                let left = tags[rng.next_in_range(tags.len() as u64) as usize];
                let right = tags[rng.next_in_range(tags.len() as u64) as usize];
                format!("{} + {}", left, right)
            }
        };

        // Property set: pick either color or display
        if rng.next_bool() {
            let (r, g, b) = color_values[rng.next_in_range(color_values.len() as u64) as usize];
            rules.push(format!("{} {{ color: rgb({}, {}, {}) }}", selector, r, g, b));
        } else {
            let display_values = ["none", "block", "inline"];
            let disp = display_values[rng.next_in_range(display_values.len() as u64) as usize];
            rules.push(format!("{} {{ display: {} }}", selector, disp));
        }
    }

    rules.join("\n")
}

/// Generate and apply a random DOM to the provided StyleEngine using the RNG.
/// Returns the projection of the computed snapshot for comparison.
fn build_random_dom_and_snapshot(rng: &mut DeterministicRng, node_count: usize) -> Vec<(NodeKey, (u8, u8, u8), Display)> {
    let mut engine = StyleEngine::new();

    // Random stylesheet with a modest number of rules to keep tests fast.
    let stylesheet_text = generate_random_stylesheet_text(rng, 12);
    let sheet = build_stylesheet(&stylesheet_text);
    engine.replace_stylesheet(sheet);

    // Track children count per parent to assign insertion positions.
    let mut child_counts: HashMap<NodeKey, usize> = HashMap::new();
    child_counts.insert(NodeKey::ROOT, 0);

    // Available parent candidates (start with ROOT). We push nodes as they are created.
    let mut parent_pool: Vec<NodeKey> = vec![NodeKey::ROOT];

    // Predefined tag/attribute pools to keep surface area controlled.
    let tags = ["div", "span", "p"];
    let class_values = ["a", "b", "c", "red", "blue"]; // known in tests
    let id_values = ["x", "y", "z"]; // simple ids
    let data_values = ["hero", "card", "note"]; // attribute values

    // Create a small tree; cap to avoid long runtime.
    let capped = node_count.min(50);
    for i in 0..capped {
        let parent_index = rng.next_in_range(parent_pool.len() as u64) as usize;
        let parent = parent_pool[parent_index];
        let node_key = NodeKey(10_000 + i as u64);
        let tag = tags[rng.next_in_range(tags.len() as u64) as usize];

        let pos = *child_counts.get(&parent).unwrap_or(&0);
        engine
            .apply_update(DOMUpdate::InsertElement { parent, node: node_key, tag: tag.to_string(), pos })
            .unwrap();
        child_counts.insert(parent, pos + 1);
        child_counts.insert(node_key, 0);

        // Randomly assign class/id/data-kind attributes
        if rng.next_bool() {
            let class_value = class_values[rng.next_in_range(class_values.len() as u64) as usize];
            engine
                .apply_update(DOMUpdate::SetAttr { node: node_key, name: "class".into(), value: class_value.into() })
                .unwrap();
        }
        if rng.next_bool() {
            let id_value = id_values[rng.next_in_range(id_values.len() as u64) as usize];
            engine
                .apply_update(DOMUpdate::SetAttr { node: node_key, name: "id".into(), value: id_value.into() })
                .unwrap();
        }
        if rng.next_bool() {
            let data_value = data_values[rng.next_in_range(data_values.len() as u64) as usize];
            engine
                .apply_update(DOMUpdate::SetAttr { node: node_key, name: "data-kind".into(), value: data_value.into() })
                .unwrap();
        }

        // Grow the parent pool with some probability to create deeper trees.
        if rng.next_bool() {
            parent_pool.push(node_key);
        }
    }

    // Flush updates to compute styles.
    engine.apply_update(DOMUpdate::EndOfDocument).unwrap();

    // Project snapshot into a stable vector sorted by NodeKey for deterministic comparisons.
    let mut snapshot: Vec<(NodeKey, (u8, u8, u8), Display)> = engine
        .computed_snapshot()
        .into_iter()
        .map(|(key, cs)| (key, (cs.color.red, cs.color.green, cs.color.blue), cs.display))
        .collect();

    snapshot.sort_by_key(|entry| entry.0 .0);
    snapshot
}

/// Ensure that generating a random DOM and stylesheet with the same seed is deterministic
/// and produces stable computed styles (projected to color + display) across runs.
#[test]
fn fuzz_determinism_across_runs_for_same_seed() {
    let _ = env_logger::builder().is_test(true).try_init();

    // A handful of seeds to exercise different shapes without slowing CI.
    let seeds: [u64; 5] = [1, 42, 12345, 7777777, 0xDEADBEEF];

    for seed in seeds {
        let mut rng1 = DeterministicRng::new(seed);
        let mut rng2 = DeterministicRng::new(seed);

        let snap1 = build_random_dom_and_snapshot(&mut rng1, 40);
        let snap2 = build_random_dom_and_snapshot(&mut rng2, 40);

        assert_eq!(snap1, snap2, "computed snapshot projection should be identical for same seed: {}", seed);
    }
}

/// Basic invariants: colors remain in byte range and display is one of known enum variants.
/// This mostly guards against panics or uninitialized values surfacing from random combinations.
#[test]
fn fuzz_basic_invariants_hold() {
    let _ = env_logger::builder().is_test(true).try_init();

    let seeds: [u64; 3] = [3, 99, 20250909];
    for seed in seeds {
        let mut rng = DeterministicRng::new(seed);
        let snapshot = build_random_dom_and_snapshot(&mut rng, 50);

        for (_key, _, display) in snapshot.into_iter() {
            // Color bytes are already u8, but we ensure no unexpected values via simple checks.
            // Display must be one of the enum variants we know; matching forces exhaustive handling.
            match display {
                Display::None | Display::Block | Display::Inline => {}
            }
        }
    }
}
