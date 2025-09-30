//! Layout comparison binary for testing against Chromium.

use env_logger::{Builder, Env};
use log::{error, info};
use std::env;
use std::process::exit;
use valor::layout_compare_core::run;

/// Parse filter argument from command line arguments.
fn parse_filter_from_args() -> Option<String> {
    let mut args = env::args();
    let _prog_name: Option<String> = args.next(); // skip program name
    let mut pending = false;
    for arg in args {
        if let Some(rest) = arg.strip_prefix("--filter=") {
            return Some(rest.to_owned());
        }
        if let Some(rest) = arg.strip_prefix("--fixture=") {
            return Some(rest.to_owned());
        }
        if let Some(rest) = arg.strip_prefix("filter=") {
            return Some(rest.to_owned());
        }
        if let Some(rest) = arg.strip_prefix("fixture=") {
            return Some(rest.to_owned());
        }
        if arg == "--filter" || arg == "--fixture" {
            pending = true;
            continue;
        }
        if pending {
            return Some(arg);
        }
    }
    None
}

fn main() {
    let _log_init: Result<(), _> = Builder::from_env(Env::default().filter_or("RUST_LOG", "warn"))
        .is_test(false)
        .try_init();
    let filter = parse_filter_from_args();
    match run(filter.as_deref()) {
        Ok(count) => {
            info!("[LAYOUT] completed: {count} fixtures passed");
        }
        Err(err) => {
            error!("error: {err}");
            exit(1);
        }
    }
}
