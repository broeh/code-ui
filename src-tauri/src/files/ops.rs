use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

pub fn root(project: &str) -> Result<PathBuf, String> {
    let path = Path::new(project)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.is_dir() {
        return Err("The project folder is unavailable.".into());
    }
    Ok(path)
}
fn relative(rel: &str) -> Result<&Path, String> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return Err("Use a relative project path.".into());
    }
    Ok(path)
}
pub fn existing(project: &str, rel: &str) -> Result<PathBuf, String> {
    let root = root(project)?;
    let path = root.join(relative(rel)?);
    let canonical = path.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&root) {
        return Err("That path leaves the project folder.".into());
    }
    Ok(path)
}
pub fn destination(project: &str, rel: &str) -> Result<PathBuf, String> {
    let root = root(project)?;
    let path = root.join(relative(rel)?);
    let parent = path
        .parent()
        .ok_or("Choose a file inside the project.")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !parent.starts_with(&root) {
        return Err("That path leaves the project folder.".into());
    }
    let name = path.file_name().ok_or("Enter a file name.")?;
    let path = parent.join(name);
    if path.exists() {
        existing(project, rel)?;
    }
    Ok(path)
}
fn name(name: &str) -> Result<(), String> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.contains('/')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("Use one file name, without slashes.".into());
    }
    Ok(())
}
pub fn stamp(path: &Path) -> Result<String, String> {
    Ok(fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string())
}
pub fn read_text(project: &str, rel: &str) -> Result<Value, String> {
    let path = existing(project, rel)?;
    let meta = path.metadata().map_err(|e| e.to_string())?;
    if meta.len() > 2 * 1024 * 1024 {
        return Ok(json!({"error":"tooLarge"}));
    }
    let mtime = stamp(&path)?;
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    if stamp(&path)? != mtime {
        return Err("File changed while reading. Try again.".into());
    }
    if bytes.contains(&0) {
        return Ok(json!({"error":"binary"}));
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(json!({"text":text,"mtime":mtime})),
        Err(_) => Ok(json!({"error":"binary"})),
    }
}
pub fn write_text(
    project: &str,
    rel: &str,
    text: &str,
    expected: Option<&str>,
) -> Result<Value, String> {
    let path = if root(project)?.join(relative(rel)?).exists() {
        existing(project, rel)?
            .canonicalize()
            .map_err(|e| e.to_string())?
    } else {
        destination(project, rel)?
    };
    if path.is_dir() {
        return Err("Choose a text file, not a folder.".into());
    }
    let current = stamp(&path).ok();
    if expected.is_some() && current.as_deref() != expected {
        return Ok(json!({"conflict":true}));
    }
    let temporary = path
        .parent()
        .ok_or("Choose a file inside the project.")?
        .join(format!(".code-ui-save-{}", uuid::Uuid::new_v4()));
    let saved = (|| -> Result<Value, String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if let Ok(meta) = fs::metadata(&path) {
            fs::set_permissions(&temporary, meta.permissions()).map_err(|e| e.to_string())?;
        }
        if expected.is_some() && stamp(&path).ok().as_deref() != expected {
            return Ok(json!({"conflict":true}));
        }
        fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        Ok(json!({"mtime":stamp(&path)?}))
    })();
    let _ = fs::remove_file(&temporary);
    saved
}
pub fn create(project: &str, dir: &str, filename: &str, folder: bool) -> Result<String, String> {
    name(filename)?;
    let parent = existing(project, dir)?;
    if !parent.is_dir() {
        return Err("Choose a folder.".into());
    }
    let path = parent.join(filename);
    if folder {
        fs::create_dir(&path).map_err(|e| e.to_string())?;
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
    }
    let root = root(project)?;
    Ok(path
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .to_string())
}
pub fn rename(project: &str, from: &str, to: &str) -> Result<(), String> {
    if from.is_empty() || to.is_empty() {
        return Err("Choose a file inside the project.".into());
    }
    let source = existing(project, from)?;
    if source.canonicalize().map_err(|e| e.to_string())? == root(project)? {
        return Err("The project root cannot be renamed.".into());
    }
    let target = destination(project, to)?;
    if target.symlink_metadata().is_ok() {
        return Err("A file or folder with that name already exists.".into());
    }
    fs::rename(source, target).map_err(|e| e.to_string())
}
pub fn trash_path(project: &str, rel: &str) -> Result<(), String> {
    if rel.is_empty() {
        return Err("The project root cannot be moved to trash.".into());
    }
    {
        let path = existing(project, rel)?;
        if path.canonicalize().map_err(|e| e.to_string())? == root(project)? {
            return Err("The project root cannot be moved to trash.".into());
        }
        trash::delete(path)
    }
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn traversal_and_symlink_escapes_are_rejected() {
        let dir = std::env::temp_dir().join(format!("code-ui-paths-{}", uuid::Uuid::new_v4()));
        let project = dir.join("project");
        let outside = dir.join("outside");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("private.txt"), "outside").unwrap();
        symlink(&outside, project.join("escape")).unwrap();
        let root = project.to_str().unwrap();
        assert!(existing(root, "../outside/private.txt").is_err());
        assert!(existing(root, "escape/private.txt").is_err());
        assert!(destination(root, "escape/new.txt").is_err());
        assert!(destination(root, "../outside/new.txt").is_err());
        assert!(create(root, "", "../overwrite", false).is_err());
        assert!(existing(root, "/etc/passwd").is_err());
        assert!(write_text(root, ".", "outside temporary file", None).is_err());
        assert!(trash_path(root, ".").is_err());
        assert!(rename(root, ".", "renamed-root").is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn saves_preserve_external_changes_and_new_files_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("code-ui-save-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let root = dir.to_str().unwrap();
        create(root, "", "notes.md", false).unwrap();
        assert!(create(root, "", "notes.md", false).is_err());
        fs::write(dir.join("notes.md"), "original").unwrap();
        let original = read_text(root, "notes.md").unwrap();
        fs::write(dir.join("notes.md"), "external update").unwrap();
        assert_eq!(
            write_text(root, "notes.md", "draft", original["mtime"].as_str()).unwrap()["conflict"],
            true
        );
        assert_eq!(
            fs::read_to_string(dir.join("notes.md")).unwrap(),
            "external update"
        );
        write_text(root, "notes.md", "draft", None).unwrap();
        assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "draft");
        assert!(rename(root, "notes.md", "notes.md").is_err());
        rename(root, "notes.md", "renamed.md").unwrap();
        assert!(dir.join("renamed.md").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
