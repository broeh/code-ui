use super::StepKind;
use serde_json::Value;
use std::path::Path;
pub fn title(tool: &str, input: &Value, cwd: &Path) -> (StepKind, String) {
    let arg = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| input[*key].as_str())
            .unwrap_or("")
            .to_string()
    };
    let path = arg(&[
        "file_path",
        "path",
        "AbsolutePath",
        "TargetFile",
        "DirectoryPath",
    ]);
    let rel = Path::new(&path)
        .strip_prefix(cwd)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(path);
    let (kind, title) = match tool {
        "Read" | "view_file" | "read_file" => (StepKind::Read, format!("Reading {rel}")),
        "Grep" | "Glob" | "grep_search" => (
            StepKind::Search,
            format!("Searching for {}", arg(&["pattern", "SearchString"])),
        ),
        "LS" | "list_dir" => (StepKind::List, format!("Listing {rel}")),
        "Edit" | "MultiEdit" | "NotebookEdit" | "replace_file_content" => {
            (StepKind::Edit, format!("Editing {rel}"))
        }
        "Write" | "write_to_file" | "write_file" => (StepKind::Write, format!("Writing {rel}")),
        "Bash" | "BashOutput" | "KillShell" | "run_command" | "bash" => (
            StepKind::Command,
            format!(
                "Running: {}",
                arg(&["description", "command", "CommandLine"])
                    .lines()
                    .next()
                    .unwrap_or_default()
            ),
        ),
        "WebFetch" | "WebSearch" => (
            StepKind::Web,
            format!("Searching the web: {}", arg(&["query", "url"])),
        ),
        "Task" | "Agent" => (
            StepKind::Subagent,
            format!("Delegating: {}", arg(&["description"])),
        ),
        "TodoWrite" | "write_todos" => (StepKind::Todo, "Updating the task list".into()),
        "Skill" => (StepKind::Skill, format!("Using skill {}", arg(&["skill"]))),
        tool if tool.starts_with("mcp__") => (
            StepKind::Mcp,
            format!(
                "Using {}",
                tool.trim_start_matches("mcp__").replace("__", " · ")
            ),
        ),
        _ => (StepKind::Other, format!("Using {tool}")),
    };
    (kind, title.chars().take(80).collect())
}
pub fn finished(title: &str) -> String {
    for (present, past) in [
        ("Reading ", "Read "),
        ("Searching for ", "Searched for "),
        ("Listing ", "Listed "),
        ("Editing ", "Edited "),
        ("Writing ", "Wrote "),
        ("Running: ", ""),
    ] {
        if let Some(rest) = title.strip_prefix(present) {
            return format!("{past}{rest}");
        }
    }
    title.into()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn tool_titles_are_relative_and_unicode_safe() {
        assert_eq!(
            title(
                "Read",
                &json!({"file_path":"/project/src/main.rs"}),
                Path::new("/project")
            ),
            (StepKind::Read, "Reading src/main.rs".into())
        );
        for tool in [
            "Grep",
            "Glob",
            "LS",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
            "Write",
            "Bash",
            "BashOutput",
            "KillShell",
            "WebFetch",
            "WebSearch",
            "Task",
            "Agent",
            "TodoWrite",
            "write_todos",
            "Skill",
            "mcp__test__run",
            "FutureTool",
        ] {
            let (_, text) = title(
                tool,
                &json!({"description":"é".repeat(100)}),
                Path::new("/project"),
            );
            assert!(text.chars().count() <= 80);
        }
        assert_eq!(finished("Reading src/main.rs"), "Read src/main.rs");
        assert_eq!(finished("Running: cargo test"), "cargo test");
    }
}
