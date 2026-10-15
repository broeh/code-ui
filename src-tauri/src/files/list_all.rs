use super::{ops, tree};
pub fn list(project: &str) -> Result<Vec<String>, String> {
    let root = ops::root(project)?;
    let mut files = Vec::new();
    for item in tree::walker(&root, false).build() {
        let item = item.map_err(|e| e.to_string())?;
        if item.path().is_file() {
            files.push(
                item.path()
                    .strip_prefix(&root)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .to_string(),
            );
            if files.len() == 50_000 {
                break;
            }
        }
    }
    files.sort();
    Ok(files)
}
