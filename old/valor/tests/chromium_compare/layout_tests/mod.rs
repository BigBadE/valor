//! Layout comparison tests against Chromium.

pub mod chromium_extraction;
pub mod serialization;
pub mod setup;

// All functionality has been moved to layout_comparison.rs

/// Macro to batch layout comparison tests.
#[macro_export]
macro_rules! layout_test_batch {
    ($test_name:ident: [$($fixture:expr),+ $(,)?]) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $test_name() -> Result<()> {
            use $crate::chromium_compare::chrome::setup_browser;
            use $crate::chromium_compare::layout_tests::compare_layout_single;
            use tokio::runtime::Handle;

            let handle = Handle::current();
            let (browser, mut chrome_pages) = setup_browser().await?;

            let mut all_failed: Vec<(String, String)> = Vec::new();
            let fixtures = vec![$($fixture),+];

            for fixture_path in fixtures {
                let display_name = fixture_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(fixture_path.to_str().unwrap_or("unknown"));

                let page = chrome_pages
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("No Chrome page available"))?;

                let failed = compare_layout_single(&page, &handle, fixture_path, display_name).await?;
                all_failed.extend(failed);

                chrome_pages.push(page);
            }

            drop(chrome_pages);
            drop(browser);

            if !all_failed.is_empty() {
                info!("\nFailed tests:");
                for (name, msg) in &all_failed {
                    info!("  {name}: {msg}");
                }
                anyhow::bail!("{} layout tests failed", all_failed.len());
            }

            Ok(())
        }
    };
}
