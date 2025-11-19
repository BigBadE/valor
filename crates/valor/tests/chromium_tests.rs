mod chromium_compare;

#[cfg(test)]
mod test {
    use super::chromium_compare;
    use anyhow::Result;

    /// Runs layout tests first, then graphics tests only if layout passes.
    ///
    /// # Errors
    ///
    /// Returns an error if layout tests fail or if graphics tests fail (when layout passes).
    #[tokio::test(flavor = "multi_thread")]
    async fn run_chromium_tests() -> Result<()> {
        chromium_compare::run_chromium_tests().await
    }
}

// Include generated fixture tests
#[cfg(test)]
include!(concat!(env!("OUT_DIR"), "/generated_fixture_tests.rs"));
