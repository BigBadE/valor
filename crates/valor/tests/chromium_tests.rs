mod chromium_compare;

// Include generated fixture tests
#[cfg(test)]
include!(concat!(env!("OUT_DIR"), "/generated_fixture_tests.rs"));
