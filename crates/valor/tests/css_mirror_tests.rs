use anyhow::Error;
use std::path::PathBuf;
use tokio::runtime::Runtime;

mod common;

fn fixture_path(name: &str) -> PathBuf {
    let p = common::fixtures_css_dir().join(name);
    if p.exists() {
        p
    } else {
        common::fixtures_dir().join(name)
    }
}

#[test]
fn css_style_block_parsing() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let path = fixture_path("style_rules.html");
    let url = common::to_file_url(&path)?;
    let rt = Runtime::new()?;
    let mut page = common::create_page(&rt, url)?;
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "Parsing did not finish for {}", path.display());

    // Snapshot collected styles from CSSMirror
    let styles = page.styles_snapshot()?;
    // Expect at least the rules we added in the <style> block
    assert!(
        styles.rules.len() >= 2,
        "Expected at least 2 parsed rules from <style>, got {}",
        styles.rules.len()
    );

    Ok(())
}

#[test]
fn css_mirror_link_discovery() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let path = fixture_path("link_discovery.html");
    let url = common::to_file_url(&path)?;
    let rt = Runtime::new()?;
    let mut page = common::create_page(&rt, url)?;
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "Parsing did not finish for {}", path.display());

    // Snapshot discovered external stylesheet links
    let links = page.discovered_stylesheets_snapshot()?;
    assert!(
        links.iter().any(|s| s.ends_with("test.css")),
        "Expected discovered href ending with 'test.css', discovered: {:?}",
        links
    );

    Ok(())
}
