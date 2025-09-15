use std::collections::{BTreeMap, HashMap};

/// In-memory string key/value store per origin.
#[derive(Debug, Default)]
pub struct StorageRegistry {
    pub(crate) buckets: HashMap<String, BTreeMap<String, String>>, // origin -> ordered key/value
}

impl StorageRegistry {
    pub fn get_bucket_mut(&mut self, origin: &str) -> &mut BTreeMap<String, String> {
        self.buckets.entry(origin.to_string()).or_default()
    }
    pub fn get_bucket(&self, origin: &str) -> Option<&BTreeMap<String, String>> {
        self.buckets.get(origin)
    }
}
