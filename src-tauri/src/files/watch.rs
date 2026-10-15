use super::{Files, ops};
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use serde_json::json;
use std::{collections::HashSet, path::Path, time::Duration};
pub struct ProjectWatch {
    pub project: String,
    _watch: Debouncer<RecommendedWatcher, RecommendedCache>,
}
fn ignored(path: &Path) -> bool {
    path.components().any(|c| {
        [".git", "node_modules", "target"]
            .iter()
            .any(|name| c.as_os_str() == *name)
    })
}
impl Files {
    pub fn watch(&self, project: &str) -> Result<(), String> {
        let mut current = self.watcher.lock().map_err(|e| e.to_string())?;
        if project.is_empty() {
            *current = None;
            return Ok(());
        }
        if !Path::new(project).is_dir() {
            *current = None;
            return Err("The project folder is not available. Choose another project or restore the folder.".into());
        }
        let root = ops::root(project)?;
        let project = root.to_string_lossy().to_string();
        if current.as_ref().is_some_and(|w| w.project == project) {
            return Ok(());
        }
        self.invalidate(&project);
        let cache = self.cache.clone();
        let sink = self.sink.clone();
        let key = project.clone();
        let mut watcher = new_debouncer(
            Duration::from_millis(300),
            None,
            move |result: DebounceEventResult| match result {
                Ok(events) => {
                    let mut dirs = HashSet::new();
                    for event in events {
                        for path in event.paths.iter() {
                            let Ok(rel) = path.strip_prefix(&root) else {
                                continue;
                            };
                            if ignored(rel) {
                                continue;
                            }
                            let parent = rel.parent().unwrap_or(Path::new(""));
                            dirs.insert(parent.to_string_lossy().to_string());
                            if path.is_dir() {
                                dirs.insert(rel.to_string_lossy().to_string());
                            }
                        }
                    }
                    if !dirs.is_empty() {
                        if let Ok(mut cache) = cache.lock() {
                            cache.remove(&key);
                        }
                        let mut dirs: Vec<_> = dirs.into_iter().collect();
                        dirs.sort();
                        sink("files-changed", json!({"project":key,"dirs":dirs}));
                    }
                }
                Err(errors) => {
                    for error in errors {
                        tracing::warn!(%error,"File watcher error");
                    }
                }
            },
        )
        .map_err(|e| e.to_string())?;
        watcher
            .watch(Path::new(&project), RecursiveMode::Recursive)
            .map_err(|e| e.to_string())?;
        *current = Some(ProjectWatch {
            project,
            _watch: watcher,
        });
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    #[test]
    fn watcher_emits_directory_changes() {
        let dir = std::env::temp_dir().join(format!("code-ui-watch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let (tx, rx) = mpsc::channel();
        let files = Files::new(Arc::new(move |_, value| {
            let _ = tx.send(value);
        }));
        files.watch(dir.to_str().unwrap()).unwrap();
        std::fs::write(dir.join("created.md"), "created").unwrap();
        let event = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(event["project"], dir.to_string_lossy().as_ref());
        assert!(event["dirs"].as_array().unwrap().iter().any(|d| d == ""));
        files.watch("").unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
