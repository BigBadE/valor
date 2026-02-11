use std::collections::{BTreeMap, HashMap};

/// In-memory string key/value store per origin.
#[derive(Debug, Default)]
pub struct StorageRegistry {
    /// Map from origin to ordered key/value pairs.
    pub buckets: HashMap<String, BTreeMap<String, String>>,
}

impl StorageRegistry {
    #[inline]
    pub fn get_bucket_mut(&mut self, origin: &str) -> &mut BTreeMap<String, String> {
        self.buckets.entry(origin.to_owned()).or_default()
    }
    #[inline]
    pub fn get_bucket(&self, origin: &str) -> Option<&BTreeMap<String, String>> {
        self.buckets.get(origin)
    }
}
