use log::{error, info};
use std::env;

fn parse_filter_from_args() -> Option<String> {
    let mut args = env::args();
    let _ = args.next(); // skip program name
    let mut pending = false;
    for arg in args {
        if let Some(rest) = arg.strip_prefix("--filter=") {
            return Some(rest.to_string());
        }
        if let Some(rest) = arg.strip_prefix("--fixture=") {
            return Some(rest.to_string());
        }
        if let Some(rest) = arg.strip_prefix("filter=") {
            return Some(rest.to_string());
        }
        if let Some(rest) = arg.strip_prefix("fixture=") {
            return Some(rest.to_string());
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
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("RUST_LOG", "warn"))
        .is_test(false)
        .try_init();
    let filter = parse_filter_from_args();
    match valor::layout_compare_core::run(filter) {
        Ok(count) => {
            info!("[LAYOUT] completed: {count} fixtures passed");
        }
        Err(err) => {
            error!("error: {err}");
            std::process::exit(1);
        }
    }
}
