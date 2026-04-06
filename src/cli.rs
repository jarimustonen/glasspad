use std::io::Read;
use std::path::PathBuf;

use crate::models::PadCreated;

const DEFAULT_PORT: u16 = 3000;
const DEFAULT_URL: &str = "http://localhost:3000";

/// Check if server is running, if not spawn it in background
async fn ensure_server(base: &str) {
    let client = reqwest::Client::new();
    if client.get(format!("{}/api/pads", base)).send().await.is_ok() {
        return;
    }

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

/// Parse a --data argument like "events=path/to/file.csv"
pub fn parse_data_arg(s: &str) -> Result<(String, PathBuf), String> {
    match s.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => {
            Ok((name.to_string(), PathBuf::from(path)))
        }
        _ => Err(format!(
            "Invalid --data format: '{}'. Expected: name=path (e.g., events=data.csv)",
            s
        )),
    }
}

pub async fn create(
    file: Option<PathBuf>,
    data_args: Vec<(String, PathBuf)>,
    base_url: Option<String>,
) {
    let base = base_url.unwrap_or_else(|| DEFAULT_URL.to_string());
    ensure_server(&base).await;

    let spec_body = match file {
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

    if data_args.is_empty() {
        // Simple YAML-only upload
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/pads", base))
            .header("Content-Type", "application/x-yaml")
            .body(spec_body)
            .send()
            .await;

        handle_create_response(resp).await;
    } else {
        // Multipart: spec + data files
        // For now, inject data into the spec as inline_data
        // (full multipart support comes with API rewrite)
        let mut spec_value: serde_yaml::Value = serde_yaml::from_str(&spec_body)
            .unwrap_or_else(|e| {
                eprintln!("Error parsing YAML: {}", e);
                std::process::exit(1);
            });

        // Read and inject each data file
        for (name, path) in &data_args {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");

            let data_str = std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("Error reading data file {}: {}", path.display(), e);
                std::process::exit(1);
            });

            let rows: serde_json::Value = match ext {
                "csv" => {
                    let dataset = crate::data::csv::parse_csv_str(&data_str)
                        .unwrap_or_else(|e| {
                            eprintln!("Error parsing CSV {}: {}", path.display(), e);
                            std::process::exit(1);
                        });
                    // Convert to JSON for injection
                    serde_json::to_value(&dataset).unwrap()
                }
                "json" => {
                    let dataset = crate::data::json::parse_json_str(&data_str)
                        .unwrap_or_else(|e| {
                            eprintln!("Error parsing JSON {}: {}", path.display(), e);
                            std::process::exit(1);
                        });
                    serde_json::to_value(&dataset).unwrap()
                }
                _ => {
                    eprintln!("Unsupported data format: {} (use .csv or .json)", ext);
                    std::process::exit(1);
                }
            };

            // Inject inline_data into sections that reference this dataset
            inject_dataset_into_spec(&mut spec_value, name, &rows);
        }

        let yaml_body = serde_yaml::to_string(&spec_value).unwrap();

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/api/pads", base))
            .header("Content-Type", "application/x-yaml")
            .body(yaml_body)
            .send()
            .await;

        handle_create_response(resp).await;
    }
}

/// Inject dataset rows into sections that reference the given source name.
fn inject_dataset_into_spec(
    spec: &mut serde_yaml::Value,
    dataset_name: &str,
    rows_json: &serde_json::Value,
) {
    // Convert JSON rows to YAML value
    let rows_yaml: serde_yaml::Value =
        serde_yaml::from_str(&serde_json::to_string(rows_json).unwrap()).unwrap();

    if let Some(sections) = spec.get_mut("sections").and_then(|s| s.as_sequence_mut()) {
        for section in sections {
            let source = section
                .get("source")
                .and_then(|s| s.as_str())
                .map(|s| s.to_string());
            if source.as_deref() == Some(dataset_name) {
                // Remove source, add inline_data
                if let Some(map) = section.as_mapping_mut() {
                    map.remove(&serde_yaml::Value::String("source".to_string()));
                    map.insert(
                        serde_yaml::Value::String("inline_data".to_string()),
                        rows_yaml.clone(),
                    );
                }
            }
        }
    }

    // Remove the dataset declaration since data is now inline
    if let Some(datasets) = spec.get_mut("datasets").and_then(|d| d.as_mapping_mut()) {
        datasets.remove(&serde_yaml::Value::String(dataset_name.to_string()));
    }
}

async fn handle_create_response(resp: Result<reqwest::Response, reqwest::Error>) {
    match resp {
        Ok(r) if r.status().is_success() => {
            let created: PadCreated = r.json().await.unwrap();
            eprintln!("Created pad {}", created.id);
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
            eprintln!("Is the glasspad server running? Start it with: glasspad serve");
            std::process::exit(1);
        }
    }
}

pub async fn list(base_url: Option<String>) {
    let base = base_url.unwrap_or_else(|| DEFAULT_URL.to_string());
    ensure_server(&base).await;

    let client = reqwest::Client::new();
    let resp = client.get(format!("{}/api/pads", base)).send().await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let pads: Vec<serde_json::Value> = r.json().await.unwrap();
            if pads.is_empty() {
                println!("No pads");
                return;
            }
            println!("{:<36} {:<40} {:<12} {}", "ID", "TITLE", "TYPE", "URL");
            for p in &pads {
                println!(
                    "{:<36} {:<40} {:<12} {}",
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
