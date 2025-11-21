use anyhow::Result;
use std::process::{Command, Stdio};
use std::time::Duration;
use std::thread;

#[test]
fn test_launch_chrome_manually() -> Result<()> {
    println!("\n=== Launching Chrome manually to see stderr ===");

    let chrome_path = "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome";

    // Launch Chrome with debugging port
    let mut child = Command::new(chrome_path)
        .args(&[
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            "--remote-debugging-port=9222",
        ])
        .stderr(Stdio::piped())
        .spawn()?;

    println!("✓ Chrome launched (PID: {})", child.id());

    // Give it a moment to start
    thread::sleep(Duration::from_secs(2));

    // Check if it's still running
    match child.try_wait()? {
        Some(status) => {
            println!("✗ Chrome exited with: {:?}", status);
            if let Some(mut stderr) = child.stderr {
                use std::io::Read;
                let mut buf = String::new();
                stderr.read_to_string(&mut buf)?;
                println!("stderr: {}", buf);
            }
            panic!("Chrome exited early");
        }
        None => {
            println!("✓ Chrome still running after 2s");

            // Try to connect with curl
            println!("\nTrying to fetch debugging info from http://localhost:9222/json...");
            let output = Command::new("curl")
                .args(&["-s", "http://localhost:9222/json"])
                .output()?;
            if output.status.success() {
                let body = String::from_utf8_lossy(&output.stdout);
                println!("✓ Got response ({}  bytes): {}", body.len(), &body[..200.min(body.len())]);
            } else {
                println!("✗ curl failed");
            }

            // Kill it
            child.kill()?;
            println!("\n✓ Killed Chrome");
        }
    }

    Ok(())
}
