use super::ops;
use ignore::WalkBuilder;
use serde::Serialize;
use std::path::Path;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub rel: String,
    pub is_dir: bool,
    pub size: u64,
    pub kind: String,
}
pub fn kind(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ["png", "jpg", "jpeg", "gif", "webp", "svg"].contains(&ext.as_str()) {
        return "image";
    }
    if [
        "pdf", "zip", "gz", "7z", "mp4", "mp3", "ogg", "wav", "woff", "woff2", "ttf", "ico", "exe",
        "db", "sqlite", "sqlite3", "wasm",
    ]
    .contains(&ext.as_str())
    {
        return "other";
    }
    "text"
}
pub fn walker(path: &Path, ignored: bool) -> WalkBuilder {
    let mut walker = WalkBuilder::new(path);
    walker
        .hidden(false)
        .require_git(false)
        .git_ignore(!ignored)
        .git_exclude(!ignored)
        .git_global(!ignored)
        .ignore(!ignored)
        .filter_entry(|e| e.file_name() != ".git");
    walker
}
pub fn list(project: &str, rel: &str, show_ignored: bool) -> Result<Vec<Entry>, String> {
    let root = ops::root(project)?;
    let path = ops::existing(project, rel)?;
    if !path.is_dir() {
        return Err("Choose a folder.".into());
    }
    let mut builder = walker(&path, show_ignored);
    builder.max_depth(Some(1));
    let mut entries = Vec::new();
    for item in builder.build() {
        let item = item.map_err(|e| e.to_string())?;
        if item.depth() == 0 {
            continue;
        }
        let path = item.path();
        let canonical = path
            .canonicalize()
            .ok()
            .filter(|path| path.starts_with(&root));
        let metadata = match &canonical {
            Some(path) => path.metadata(),
            None => path.symlink_metadata(),
        }
        .map_err(|e| e.to_string())?;
        entries.push(Entry {
            name: item.file_name().to_string_lossy().to_string(),
            rel: path
                .strip_prefix(&root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .to_string(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            kind: if metadata.is_dir() {
                "folder".into()
            } else {
                if canonical.is_none() {
                    "other".into()
                } else {
                    kind(path).into()
                }
            },
        });
    }
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries.truncate(2000);
    Ok(entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gitignore_and_hidden_files_work_without_git_init() {
        let dir = std::env::temp_dir().join(format!("code-ui-tree-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::create_dir(dir.join("folder")).unwrap();
        std::fs::create_dir(dir.join(".git")).unwrap();
        for (name, text) in [
            (".gitignore", "ignored.txt\n"),
            ("visible.md", "visible"),
            ("ignored.txt", "ignored"),
            (".env.example", "example"),
        ] {
            std::fs::write(dir.join(name), text).unwrap();
        }
        let rows = list(dir.to_str().unwrap(), "", false).unwrap();
        assert!(rows[0].is_dir);
        assert!(rows.iter().any(|r| r.name == ".env.example"));
        assert!(!rows.iter().any(|r| r.name == "ignored.txt"));
        let rows = list(dir.to_str().unwrap(), "", true).unwrap();
        assert!(rows.iter().any(|r| r.name == "ignored.txt"));
        assert!(!rows.iter().any(|r| r.name == ".git"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
