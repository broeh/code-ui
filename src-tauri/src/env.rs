use crate::settings::Settings;
use serde::Serialize;
use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::{process::Command, time::timeout};

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    pub agent: String,
    pub installed: bool,
    pub enabled: bool,
    pub problem: Option<String>,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Clone, Default)]
pub struct Environment {
    pub vars: HashMap<String, String>,
    pub agents: Vec<AgentStatus>,
}

impl Environment {
    pub async fn capture(settings: &Settings) -> Self {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        let output = timeout(
            Duration::from_secs(5),
            Command::new(&shell)
                .args(["-lc", "env -0"])
                .kill_on_drop(true)
                .output(),
        )
        .await;
        let mut vars: HashMap<String, String> = match output {
            Ok(Ok(out)) if out.status.success() => parse_env(&out.stdout),
            _ => std::env::vars().collect(),
        };
        let local_bin = dirs::home_dir().unwrap_or_default().join(".local/bin");
        let path = vars.get("PATH").cloned().unwrap_or_default();
        vars.insert("PATH".into(), format!("{path}:{}", local_bin.display()));

        Self::detect(settings, vars).await
    }

    pub async fn detect(settings: &Settings, vars: HashMap<String, String>) -> Self {
        let mut tasks = Vec::new();
        for agent in ["claude", "codex", "agy", "muse"] {
            let override_path = settings.binaries.get(agent).and_then(|p| p.as_deref());
            let path = resolve_binary(agent, override_path, &vars);
            let vars = vars.clone();
            let enabled = settings
                .enabled_agents
                .iter()
                .any(|kind| kind.as_str() == agent);
            tasks.push(tokio::spawn(async move {
                let version = if let Some(binary) = &path
                    && enabled
                {
                    match timeout(
                        Duration::from_secs(5),
                        Command::new(binary)
                            .arg("--version")
                            .env_clear()
                            .envs(vars)
                            .kill_on_drop(true)
                            .output(),
                    )
                    .await
                    {
                        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                            .lines()
                            .find(|line| !line.trim().is_empty())
                            .map(|line| line.trim().to_owned()),
                        _ => None,
                    }
                } else {
                    None
                };
                AgentStatus {
                    agent: agent.into(),
                    installed: path.is_some(),
                    enabled,
                    problem: if path.is_none() {
                        Some(
                            "CLI executable not found. Install it or choose its executable path."
                                .into(),
                        )
                    } else if enabled && version.is_none() {
                        Some(
                            "The CLI did not return a version. Check the selected executable."
                                .into(),
                        )
                    } else {
                        None
                    },
                    path: path.map(|p| p.to_string_lossy().into_owned()),
                    version,
                }
            }));
        }
        let mut agents = Vec::new();
        for task in tasks {
            agents.push(task.await.expect("agent detection task panicked"));
        }
        Self { vars, agents }
    }
}

pub struct SharedEnvironment(RwLock<Arc<Environment>>);

impl From<Environment> for SharedEnvironment {
    fn from(environment: Environment) -> Self {
        Self(RwLock::new(Arc::new(environment)))
    }
}

impl SharedEnvironment {
    pub fn get(&self) -> Arc<Environment> {
        self.0.read().expect("environment lock poisoned").clone()
    }
    pub fn replace(&self, environment: Environment) {
        *self.0.write().expect("environment lock poisoned") = Arc::new(environment);
    }
    pub fn agents(&self) -> Vec<AgentStatus> {
        self.get().agents.clone()
    }
    pub fn vars(&self) -> HashMap<String, String> {
        self.get().vars.clone()
    }
    pub fn binary(&self, agent: &str) -> Option<String> {
        self.get()
            .agents
            .iter()
            .find(|status| {
                status.agent == agent
                    && status.enabled
                    && status.installed
                    && status.problem.is_none()
            })
            .and_then(|status| status.path.clone())
    }
}

fn parse_env(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            let entry = std::str::from_utf8(entry).ok()?;
            let (name, value) = entry.split_once('=')?;
            Some((name.into(), value.into()))
        })
        .collect()
}

fn executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

pub fn resolve_binary(
    name: &str,
    override_path: Option<&str>,
    vars: &HashMap<String, String>,
) -> Option<PathBuf> {
    if let Some(path) = override_path {
        let path = PathBuf::from(path);
        return (path.is_absolute() && executable(&path)).then_some(path);
    }
    // Resolving executable PATH entries is equivalent to `command -v` for these CLI names.
    std::env::split_paths(vars.get("PATH").map(String::as_str).unwrap_or_default())
        .map(|dir| dir.join(name))
        .find(|path| executable(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detection_handles_one_cli_missing_clis_and_installation_after_startup() {
        let dir = std::env::temp_dir().join(format!("code-ui-detection-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let vars = HashMap::from([("PATH".into(), dir.to_string_lossy().into_owned())]);
        let settings = Settings::default();
        let shared = SharedEnvironment::from(Environment::detect(&settings, vars.clone()).await);
        assert!(shared.binary("claude").is_none());
        let binary = dir.join("codex");
        std::fs::write(&binary, "#!/bin/sh\nprintf 'codex-cli test-version\\n'\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        shared.replace(Environment::detect(&settings, vars).await);
        assert_eq!(
            shared.binary("codex"),
            Some(binary.to_string_lossy().into_owned())
        );
        assert!(shared.binary("claude").is_none());
        assert!(
            shared
                .agents()
                .iter()
                .find(|agent| agent.agent == "codex")
                .unwrap()
                .problem
                .is_none()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn nul_environment_preserves_equals_and_newlines() {
        let vars = parse_env(b"PATH=/usr/bin\0VALUE=a=b\nc\0");
        assert_eq!(vars["VALUE"], "a=b\nc");
        assert_eq!(
            resolve_binary("sh", None, &vars),
            Some(PathBuf::from("/usr/bin/sh"))
        );
        assert!(resolve_binary("sh", Some("relative/sh"), &vars).is_none());
        assert!(resolve_binary("sh", Some("/missing/sh"), &vars).is_none());
    }
}
