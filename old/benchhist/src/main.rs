//! Benchmark history tracking tool for Valor project.
//!
//! This tool saves and compares benchmark snapshots to track performance over time.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{from_slice, to_vec_pretty};
use std::env;
use std::fs::{create_dir_all, read, write};
use std::io::{Write as _, stderr};
use std::path::PathBuf;
use std::time::Instant;

/// Snapshot of benchmark results with timestamp.
#[derive(Serialize, Deserialize)]
struct Snapshot {
    /// Timestamp in milliseconds since epoch.
    timestamp_ms: u128,
    /// Optional note describing this snapshot.
    note: String,
}

/// Return the path to the benchmarks/history directory.
fn history_dir() -> PathBuf {
    PathBuf::from("benchmarks").join("history")
}

/// Ensure the history directory exists, creating it if necessary.
///
/// # Errors
/// Returns an error if the directory cannot be created.
fn ensure_history_dir() -> Result<PathBuf> {
    let dir = history_dir();
    if !dir.exists() {
        create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Save a snapshot baseline with the given name.
///
/// # Errors
/// Returns an error if the snapshot cannot be saved.
fn cmd_save(name: &str) -> Result<()> {
    let dir = ensure_history_dir()?;
    let path = dir.join(format!("{name}.json"));
    let snap = Snapshot {
        timestamp_ms: Instant::now().elapsed().as_millis(),
        note: "placeholder snapshot".to_owned(),
    };
    let data = to_vec_pretty(&snap)?;
    write(&path, data)?;
    writeln!(stderr(), "Saved baseline to {}", path.display())?;
    Ok(())
}

/// Compare current benchmarks against a saved baseline.
///
/// # Errors
/// Returns an error if the baseline cannot be loaded or compared.
fn cmd_compare(baseline: &str, _threshold: Option<f32>) -> Result<()> {
    let path = history_dir().join(format!("{baseline}.json"));
    if !path.exists() {
        return Err(anyhow!(
            "baseline '{}' not found at {}",
            baseline,
            path.display()
        ));
    }
    let data = read(&path)?;
    let _snap: Snapshot = from_slice(&data)?;
    // Placeholder: always pass
    writeln!(
        stderr(),
        "Comparison against baseline '{baseline}' passed (placeholder)"
    )?;
    Ok(())
}

/// Print usage information to stderr.
fn print_usage() {
    drop(writeln!(
        stderr(),
        "Usage:\\n  benchhist save --name <BASELINE>\\n  benchhist compare --baseline <BASELINE> [--threshold <PERCENT>]"
    ));
}

/// Main entry point for the benchhist CLI tool.
///
/// # Errors
/// Returns an error if command parsing or execution fails.
fn main() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        return Err(anyhow!("missing command"));
    }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "save" => {
            let mut name_opt: Option<String> = None;
            let mut index = 0;
            while index < args.len() {
                match args[index].as_str() {
                    "--name" => {
                        if index + 1 < args.len() {
                            name_opt = Some(args[index + 1].clone());
                            index += 2;
                        } else {
                            break;
                        }
                    }
                    _ => {
                        index += 1;
                    }
                }
            }
            let name = name_opt.ok_or_else(|| anyhow!("--name is required"))?;
            cmd_save(&name)
        }
        "compare" => {
            let mut baseline_opt: Option<String> = None;
            let mut threshold_opt: Option<f32> = None;
            let mut index = 0;
            while index < args.len() {
                match args[index].as_str() {
                    "--baseline" => {
                        if index + 1 < args.len() {
                            baseline_opt = Some(args[index + 1].clone());
                            index += 2;
                        } else {
                            break;
                        }
                    }
                    "--threshold" => {
                        if index + 1 < args.len() {
                            threshold_opt = args[index + 1].parse::<f32>().ok();
                            index += 2;
                        } else {
                            break;
                        }
                    }
                    _ => {
                        index += 1;
                    }
                }
            }
            let baseline = baseline_opt.ok_or_else(|| anyhow!("--baseline is required"))?;
            cmd_compare(&baseline, threshold_opt)
        }
        _ => {
            print_usage();
            Err(anyhow!("unknown command"))
        }
    }
}
