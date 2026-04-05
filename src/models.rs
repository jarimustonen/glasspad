use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Top-level YAML dashboard spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSpec {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub layout: Option<String>,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    #[serde(rename = "type")]
    pub section_type: String,
    #[serde(default)]
    pub chart: Option<ChartSpec>,
    #[serde(default)]
    pub columns: Option<Vec<Column>>,
    #[serde(default)]
    pub data: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSpec {
    pub mark: String,
    #[serde(default)]
    pub direction: Option<String>,
    pub data: Vec<serde_json::Value>,
    pub encoding: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub field: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
}

/// Stored pad
#[derive(Debug, Clone)]
pub struct Pad {
    pub id: String,
    pub title: String,
    pub pad_type: String,
    pub content: PadContent,
    pub created_at: DateTime<Utc>,
    pub raw_yaml: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PadContent {
    Dashboard(DashboardSpec),
    RawHtml(String),
}

/// JSON response for pad metadata
#[derive(Debug, Serialize)]
pub struct PadMeta {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub pad_type: String,
    pub url: String,
    pub created_at: DateTime<Utc>,
}

/// JSON response for pad creation
#[derive(Debug, Serialize, Deserialize)]
pub struct PadCreated {
    pub id: String,
    pub url: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
}
