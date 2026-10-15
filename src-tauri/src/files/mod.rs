pub mod list_all;
pub mod ops;
pub mod tree;
pub mod watch;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
pub type EventSink = Arc<dyn Fn(&str, Value) + Send + Sync>;
pub struct Files {
    pub cache: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pub watcher: Mutex<Option<watch::ProjectWatch>>,
    pub sink: EventSink,
}
impl Files {
    pub fn new(sink: EventSink) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            watcher: Mutex::new(None),
            sink,
        }
    }
    pub fn all(&self, project: &str) -> Result<Vec<String>, String> {
        let project = ops::root(project)?.to_string_lossy().to_string();
        if let Some(files) = self
            .cache
            .lock()
            .map_err(|e| e.to_string())?
            .get(&project)
            .cloned()
        {
            return Ok(files);
        }
        let files = list_all::list(&project)?;
        self.cache
            .lock()
            .map_err(|e| e.to_string())?
            .insert(project, files.clone());
        Ok(files)
    }
    pub fn invalidate(&self, project: &str) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(project);
        }
    }
}
