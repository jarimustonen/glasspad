use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::data::types::{Dataset, DatasetMeta};
use crate::spec::schema::DashboardSpec;

/// Stored pad — uses the new canonical spec schema.
#[derive(Debug, Clone)]
pub struct Pad {
    pub id: String,
    pub token: String,
    pub title: String,
    pub spec: DashboardSpec,
    pub datasets: BTreeMap<String, Dataset>,
    pub dataset_meta: BTreeMap<String, DatasetMeta>,
    pub created_at: DateTime<Utc>,
}

/// JSON response for pad metadata.
#[derive(Debug, Serialize)]
pub struct PadMeta {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub pad_type: String,
    pub url: String,
    pub created_at: DateTime<Utc>,
}

/// JSON response for pad creation.
#[derive(Debug, Serialize, Deserialize)]
pub struct PadCreated {
    pub id: String,
    pub url: String,
    pub title: String,
    pub token: String,
    pub created_at: DateTime<Utc>,
}
