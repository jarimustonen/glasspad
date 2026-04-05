use std::io::Read;
use std::path::PathBuf;

use crate::models::PadCreated;

const DEFAULT_PORT: u16 = 3000;
const DEFAULT_URL: &str = "http://localhost:3000";

/// Check if server is running, if not spawn it in background
async fn ensure_server(base: &str) {
    let client = reqwest::Client::new();
    if client.get(format!("{}/api/pads", base)).send().await.is_ok() {
        return; // already running
    }

    // Spawn server as detached background process
    let exe = std::env::current_exe().unwrap();
    let port = DEFAULT_PORT.to_string();
    match std::process::Command::new(exe)
        .args(["serve", "--port", &port])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => {
            eprintln!("Starting glasspad server on port {}...", DEFAULT_PORT);
            // Wait for server to be ready
            for _ in 0..20 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if client.get(format!("{}/api/pads", base)).send().await.is_ok() {
                    return;
                }
            }
            eprintln!("Warning: server may not be ready yet");
        }
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}

pub async fn create(file: Option<PathBuf>, base_url: Option<String>) {
    let base = base_url.unwrap_or_else(|| DEFAULT_URL.to_string());
    ensure_server(&base).await;

    let body = match file {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("Error reading file {}: {}", path.display(), e);
            std::process::exit(1);
        }),
        None => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(1);
            });
            buf
        }
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/pads", base))
        .header("Content-Type", "application/x-yaml")
        .body(body)
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let created: PadCreated = r.json().await.unwrap();
            println!("Created pad {}", created.id);
            println!("{}", created.url);
        }
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            eprintln!("Error {}: {}", status, text);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Connection error: {}", e);
            std::process::exit(1);
        }
    }
}

pub async fn list(base_url: Option<String>) {
    let base = base_url.unwrap_or_else(|| DEFAULT_URL.to_string());
    ensure_server(&base).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/pads", base))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let pads: Vec<serde_json::Value> = r.json().await.unwrap();
            if pads.is_empty() {
                println!("No pads");
                return;
            }
            println!("{:<10} {:<40} {:<12} {}", "ID", "TITLE", "TYPE", "URL");
            for p in &pads {
                println!(
                    "{:<10} {:<40} {:<12} {}",
                    p["id"].as_str().unwrap_or(""),
                    p["title"].as_str().unwrap_or(""),
                    p["type"].as_str().unwrap_or(""),
                    p["url"].as_str().unwrap_or(""),
                );
            }
        }
        Ok(r) => {
            eprintln!("Error: {}", r.status());
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Connection error: {}", e);
            std::process::exit(1);
        }
    }
}

pub async fn open(id: String, base_url: Option<String>) {
    let base = base_url.unwrap_or_else(|| DEFAULT_URL.to_string());
    let url = format!("{}/{}", base, id);

    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }

    println!("Opening {}", url);
}

pub fn skill(install_claude: bool, user: bool) {
    let skill_content = include_str!("skill.md");

    if install_claude || user {
        let base = if user {
            dirs::home_dir()
                .expect("Cannot determine home directory")
                .join(".claude")
        } else {
            let claude_dir = PathBuf::from(".claude");
            if !claude_dir.exists() {
                eprintln!("Error: .claude/ directory not found in current directory");
                eprintln!("Are you in a project root? Use --user for user-level install.");
                std::process::exit(1);
            }
            claude_dir
        };

        let dir = base.join("skills/glasspad");
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| {
            eprintln!("Error creating directory: {}", e);
            std::process::exit(1);
        });
        let path = dir.join("SKILL.md");
        std::fs::write(&path, skill_content).unwrap_or_else(|e| {
            eprintln!("Error writing skill: {}", e);
            std::process::exit(1);
        });
        println!("Installed skill to {}", path.display());
    } else {
        print!("{}", skill_content);
    }
}
