use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn parser_test() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();

    let fixtures = common::fixture_html_files()?;
    assert!(!fixtures.is_empty(), "No HTML fixtures found in tests/fixtures");

    for path in fixtures {
        let url = common::to_file_url(&path)?;

        // Create a Tokio runtime and page, then loop update() until finished (with a timeout)
        let rt = Runtime::new()?;
        let mut page = common::create_page(&rt, url)?;
        let finished = common::update_until_finished_simple(&rt, &mut page)?;

        assert!(finished, "Parsing did not finish within the allotted iterations for {}", path.display());
    }

    Ok(())
}
