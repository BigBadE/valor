use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    timestamp_ms: u128,
    note: String,
}

fn history_dir() -> PathBuf {
    let mut p = PathBuf::from("benchmarks");
    p.push("history");
    p
}

fn ensure_history_dir() -> Result<PathBuf> {
    let dir = history_dir();
    if !dir.exists() { fs::create_dir_all(&dir)?; }
    Ok(dir)
}

fn cmd_save(name: &str) -> Result<()> {
    let dir = ensure_history_dir()?;
    let mut path = dir;
    path.push(format!("{}.json", name));
    let snap = Snapshot { timestamp_ms: std::time::Instant::now().elapsed().as_millis(), note: "placeholder snapshot".to_string() };
    let data = serde_json::to_vec_pretty(&snap)?;
    fs::write(&path, data)?;
    println!("Saved baseline to {}", path.display());
    Ok(())
}

fn cmd_compare(baseline: &str, _threshold: Option<f32>) -> Result<()> {
    let mut path = history_dir();
    path.push(format!("{}.json", baseline));
    if !path.exists() {
        return Err(anyhow!("baseline '{}' not found at {}", baseline, path.display()));
    }
    let data = fs::read(&path)?;
    let _snap: Snapshot = serde_json::from_slice(&data)?;
    // Placeholder: always pass
    println!("Comparison against baseline '{}' passed (placeholder)", baseline);
    Ok(())
}

fn print_usage() {
    eprintln!("Usage:\n  benchhist save --name <BASELINE>\n  benchhist compare --baseline <BASELINE> [--threshold <PERCENT>]");
}

fn main() -> Result<()> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() { print_usage(); return Err(anyhow!("missing command")); }
    let cmd = args.remove(0);
    match cmd.as_str() {
        "save" => {
            let mut name_opt: Option<String> = None;
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--name" => { if i + 1 < args.len() { name_opt = Some(args[i+1].clone()); i += 2; } else { break; } }
                    _ => { i += 1; }
                }
            }
            let name = name_opt.ok_or_else(|| anyhow!("--name is required"))?;
            cmd_save(&name)
        }
        "compare" => {
            let mut baseline_opt: Option<String> = None;
            let mut threshold_opt: Option<f32> = None;
            let mut i = 0;
            while i < args.len() {
                match args[i].as_str() {
                    "--baseline" => { if i + 1 < args.len() { baseline_opt = Some(args[i+1].clone()); i += 2; } else { break; } }
                    "--threshold" => { if i + 1 < args.len() { threshold_opt = args[i+1].parse::<f32>().ok(); i += 2; } else { break; } }
                    _ => { i += 1; }
                }
            }
            let baseline = baseline_opt.ok_or_else(|| anyhow!("--baseline is required"))?;
            cmd_compare(&baseline, threshold_opt)
        }
        _ => { print_usage(); Err(anyhow!("unknown command")) }
    }
}
