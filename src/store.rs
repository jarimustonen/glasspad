use std::collections::HashMap;
use std::sync::RwLock;

use crate::models::{Pad, PadMeta};

pub struct PadStore {
    pads: RwLock<HashMap<String, Pad>>,
    pub base_url: String,
}

impl PadStore {
    pub fn new(port: u16) -> Self {
        Self {
            pads: RwLock::new(HashMap::new()),
            base_url: format!("http://localhost:{}", port),
        }
    }

    pub fn insert(&self, pad: Pad) {
        let mut pads = self.pads.write().unwrap();
        pads.insert(pad.id.clone(), pad);
    }

    pub fn get(&self, id: &str) -> Option<Pad> {
        let pads = self.pads.read().unwrap();
        pads.get(id).cloned()
    }

    pub fn list(&self) -> Vec<PadMeta> {
        let pads = self.pads.read().unwrap();
        let mut list: Vec<PadMeta> = pads
            .values()
            .map(|p| PadMeta {
                id: p.id.clone(),
                title: p.title.clone(),
                pad_type: "dashboard".to_string(),
                url: format!("{}/{}", self.base_url, p.id),
                created_at: p.created_at,
            })
            .collect();
        list.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        list
    }

    pub fn update(&self, id: &str, pad: Pad) -> bool {
        let mut pads = self.pads.write().unwrap();
        if pads.contains_key(id) {
            pads.insert(id.to_string(), pad);
            true
        } else {
            false
        }
    }

    pub fn delete(&self, id: &str) -> bool {
        let mut pads = self.pads.write().unwrap();
        pads.remove(id).is_some()
    }
}
