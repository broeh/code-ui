use crate::db::{Database, now_ms};
use rusqlite::params;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRow {
    pub path: String,
    pub name: String,
    pub pinned: bool,
    pub extra: bool,
    pub last_activity: i64,
    pub working: i64,
    pub needs_input: i64,
}
fn canonical_folder(path: &Path) -> Result<PathBuf, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("Choose a folder.".into());
    }
    Ok(path)
}
pub fn list(db: &Database, root: &Path) -> Result<Vec<ProjectRow>, String> {
    let mut paths = HashSet::new();
    if root.exists() {
        for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir() {
                paths.insert(canonical_folder(&entry.path())?);
            }
        }
    }
    let c = db.0.lock().map_err(|e| e.to_string())?;
    let mut q = c
        .prepare("SELECT path,pinned,hidden,extra,last_opened_at FROM projects")
        .map_err(|e| e.to_string())?;
    let metadata = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, bool>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                ),
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|e| e.to_string())?;
    for (path, (_, hidden, extra, _)) in &metadata {
        if *extra && !hidden && Path::new(path).is_dir() {
            paths.insert(canonical_folder(Path::new(path))?);
        }
    }
    let mut rows = Vec::new();
    for path in paths {
        let key = path.to_string_lossy().to_string();
        let (pinned, hidden, extra, _opened) = metadata.get(&key).copied().unwrap_or_default();
        if hidden {
            continue;
        }
        let mtime = path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let (chat_time,working,needs_input):(Option<i64>,i64,i64)=c.query_row("SELECT MAX(updated_at),COALESCE(SUM(status='working'),0),COALESCE(SUM(status='needs_input'),0) FROM chats WHERE project_path=?1 AND hidden=0",[&key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
        let native:Option<i64>=c.query_row("SELECT MAX(updated_at) FROM native_sessions WHERE cwd=?1 OR substr(cwd,1,length(?1)+1)=?1||'/'",[&key],|r|r.get(0)).map_err(|e|e.to_string())?;
        rows.push(ProjectRow {
            name: path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .to_string(),
            path: key,
            pinned,
            extra,
            last_activity: mtime.max(chat_time.unwrap_or(0)).max(native.unwrap_or(0)),
            working,
            needs_input,
        });
    }
    rows.sort_by(|a, b| {
        b.pinned.cmp(&a.pinned).then_with(|| {
            if a.pinned {
                a.name.to_lowercase().cmp(&b.name.to_lowercase())
            } else {
                b.last_activity
                    .cmp(&a.last_activity)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }
        })
    });
    Ok(rows)
}
fn name_is_valid(name: &str) -> bool {
    let mut components = Path::new(name).components();
    !name.trim().is_empty()
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && name != "."
        && name != ".."
        && !name.contains('/')
}
pub fn create(db: &Database, root: &Path, name: &str) -> Result<String, String> {
    let name = name.trim();
    if !name_is_valid(name) {
        return Err("Use one folder name, without slashes.".into());
    }
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let root = canonical_folder(root)?;
    let path = root.join(name);
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    let path = path.to_string_lossy().to_string();
    touch(db, &path)?;
    Ok(path)
}
pub fn add(db: &Database, path: &str) -> Result<String, String> {
    let path = canonical_folder(Path::new(path))?
        .to_string_lossy()
        .to_string();
    db.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO projects(path,extra,hidden,last_opened_at) VALUES(?1,1,0,?2) ON CONFLICT(path) DO UPDATE SET extra=1,hidden=0,last_opened_at=excluded.last_opened_at",params![path,now_ms()]).map_err(|e|e.to_string())?;
    Ok(path)
}
pub fn touch(db: &Database, path: &str) -> Result<(), String> {
    let path = canonical_folder(Path::new(path))?
        .to_string_lossy()
        .to_string();
    db.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO projects(path,last_opened_at) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET last_opened_at=excluded.last_opened_at",params![path,now_ms()]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn set_pinned(db: &Database, path: &str, pinned: bool) -> Result<(), String> {
    let path = canonical_folder(Path::new(path))?
        .to_string_lossy()
        .to_string();
    db.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO projects(path,pinned) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET pinned=excluded.pinned",params![path,pinned]).map_err(|e|e.to_string())?;
    Ok(())
}
pub fn hide(db: &Database, path: &str) -> Result<(), String> {
    let path = canonical_folder(Path::new(path))?
        .to_string_lossy()
        .to_string();
    db.0.lock().map_err(|e|e.to_string())?.execute("INSERT INTO projects(path,hidden) VALUES(?1,1) ON CONFLICT(path) DO UPDATE SET hidden=1",[path]).map_err(|e|e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn folders_sort_filter_and_create_safely() {
        let base = std::env::temp_dir().join(format!("code-ui-projects-{}", uuid::Uuid::new_v4()));
        let root = base.join("projects");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        for name in ["Alpha", "Beta", ".hidden"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }
        std::fs::write(root.join("loose.txt"), "loose").unwrap();
        let db = Database::open(&base.join("app.db")).unwrap();
        assert_eq!(list(&db, &root).unwrap().len(), 2);
        add(&db, &outside.to_string_lossy()).unwrap();
        set_pinned(&db, &root.join("Beta").to_string_lossy(), true).unwrap();
        assert_eq!(list(&db, &root).unwrap()[0].name, "Beta");
        hide(&db, &root.join("Alpha").to_string_lossy()).unwrap();
        assert_eq!(list(&db, &root).unwrap().len(), 2);
        assert!(create(&db, &root, "../escape").is_err());
        assert!(create(&db, &root, "Beta").is_err());
        let new = create(&db, &root, "New project").unwrap();
        assert!(Path::new(&new).is_dir());
        assert!(!Path::new(&new).join(".git").exists());
        let fresh = base.join("not/yet/there");
        assert!(Path::new(&create(&db, &fresh, "First").unwrap()).is_dir());
        std::fs::remove_dir_all(base).unwrap();
    }
}
