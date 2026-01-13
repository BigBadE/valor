// Direct layout test without Chrome comparison
use std::path::PathBuf;
use tokio::runtime::Runtime;

#[path = "crates/valor/tests/chromium_compare/mod.rs"]
mod chromium_compare;

#[path = "crates/valor/tests/chromium_compare/common.rs"]
mod common;

#[path = "crates/valor/tests/chromium_compare/valor.rs"]
mod valor;

use chromium_compare::common::init_test_logger;
use chromium_compare::valor::build_layout_for_fixture;

fn main() {
    init_test_logger();

    let rt = Runtime::new().unwrap();
    let handle = rt.handle().clone();

    let fixture_path = PathBuf::from("crates/css/modules/backgrounds_borders/tests/fixtures/layout/basics/06_padding_and_border.html");

    rt.block_on(async {
        println!("Building layout for fixture: {}", fixture_path.display());

        match build_layout_for_fixture(&handle, &fixture_path).await {
            Ok(layout_json) => {
                println!("Layout JSON:\n{}", serde_json::to_string_pretty(&layout_json).unwrap());
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    });
}
