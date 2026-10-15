use crate::agents::{AgentKind, Permission};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{Mutex, RwLock},
};

pub const AGENTS: [AgentKind; 4] = [
    AgentKind::Claude,
    AgentKind::Codex,
    AgentKind::Agy,
    AgentKind::Muse,
];

fn expand_home(path: &str, home: &Path) -> String {
    match path {
        "~" => home.to_string_lossy().into_owned(),
        _ => path
            .strip_prefix("~/")
            .map(|rest| home.join(rest).to_string_lossy().into_owned())
            .unwrap_or_else(|| path.into()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub schema_version: u32,
    pub projects_root: String,
    pub binaries: HashMap<String, Option<String>>,
    pub enabled_agents: Vec<AgentKind>,
    pub default_agent: AgentKind,
    pub default_permission: Permission,
    pub history_agents: Vec<AgentKind>,
    pub trusted_projects: Vec<String>,
    pub setup_complete: bool,
    pub notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            projects_root: dirs::home_dir()
                .unwrap_or_default()
                .join("Projects")
                .to_string_lossy()
                .into_owned(),
            binaries: AGENTS
                .into_iter()
                .map(|agent| (agent.as_str().into(), None))
                .collect(),
            enabled_agents: vec![AgentKind::Claude, AgentKind::Codex],
            default_agent: AgentKind::Claude,
            default_permission: Permission::Ask,
            history_agents: Vec::new(),
            trusted_projects: Vec::new(),
            setup_complete: false,
            notifications: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            crate::privacy::private_dir(parent)?;
        }
        if !path.exists() {
            let settings = Self::default();
            persist(path, &settings)?;
            return Ok(settings);
        }
        crate::privacy::private_file(path)?;
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        let legacy = value.get("schemaVersion").is_none() && value.get("projectsRoot").is_some();
        let mut settings: Self = serde_json::from_value(value)?;
        if settings.schema_version != 1 {
            return Err(
                "This settings version is not supported. The original file has been preserved."
                    .into(),
            );
        }
        if legacy {
            settings.enabled_agents = AGENTS.to_vec();
            settings.history_agents = AGENTS.to_vec();
            settings.default_permission = Permission::Yolo;
            settings.setup_complete = true;
        }
        settings.expand_paths();
        Ok(settings)
    }

    fn expand_paths(&mut self) {
        if let Some(home) = dirs::home_dir() {
            self.projects_root = expand_home(self.projects_root.trim(), &home);
            for path in self.binaries.values_mut().flatten() {
                *path = expand_home(path.trim(), &home);
            }
        }
    }

    fn validate(&mut self, create_root: bool) -> Result<(), String> {
        self.expand_paths();
        let root = Path::new(&self.projects_root);
        if !root.is_absolute() {
            return Err("Choose an absolute projects folder path.".into());
        }
        if !self.enabled_agents.is_empty() && !self.enabled_agents.contains(&self.default_agent) {
            return Err("Choose an enabled default agent.".into());
        }
        if !matches!(self.default_permission, Permission::Ask | Permission::Yolo) {
            return Err("Choose Ask or Full access as the default permission.".into());
        }
        for path in self.binaries.values().flatten() {
            if !Path::new(path).is_absolute() {
                return Err("Use an absolute CLI executable path, or leave it empty for automatic detection.".into());
            }
        }
        if !root.exists() && create_root {
            fs::create_dir_all(root).map_err(|e| e.to_string())?;
        }
        if !root.is_dir() {
            return Err("Choose an existing projects folder, or select Create this folder.".into());
        }
        self.projects_root = root
            .canonicalize()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        self.history_agents
            .retain(|agent| self.enabled_agents.contains(agent));
        self.schema_version = 1;
        Ok(())
    }
}

fn persist(path: &Path, settings: &Settings) -> Result<(), String> {
    let parent = path.parent().ok_or("No settings directory")?;
    crate::privacy::private_dir(parent).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(".settings-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temporary, path).map_err(|e| e.to_string())
    })();
    let _ = fs::remove_file(temporary);
    result
}

pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
    error: Mutex<Option<String>>,
    writes: Mutex<()>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub settings: Settings,
    pub error: Option<String>,
    pub profile: String,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let (settings, error) = match Settings::load(&path) {
            Ok(settings) => (settings, None),
            Err(error) => (
                Settings::default(),
                Some(format!("Could not read settings: {error}")),
            ),
        };
        Self {
            path,
            current: RwLock::new(settings),
            error: Mutex::new(error),
            writes: Mutex::new(()),
        }
    }

    pub fn get(&self) -> Settings {
        self.current.read().expect("settings lock poisoned").clone()
    }

    pub fn snapshot(&self) -> SettingsSnapshot {
        SettingsSnapshot {
            settings: self.get(),
            error: self
                .error
                .lock()
                .expect("settings error lock poisoned")
                .clone(),
            profile: crate::paths::profile().into(),
        }
    }

    pub fn save(&self, mut settings: Settings, create_root: bool) -> Result<(), String> {
        settings.validate(create_root)?;
        let _guard = self.writes.lock().map_err(|e| e.to_string())?;
        for path in self.get().trusted_projects {
            if !settings.trusted_projects.contains(&path) {
                settings.trusted_projects.push(path);
            }
        }
        if self.error.lock().map_err(|e| e.to_string())?.is_some() && self.path.exists() {
            let backup = self
                .path
                .with_file_name(format!("settings.invalid-{}.json", uuid::Uuid::new_v4()));
            fs::copy(&self.path, &backup).map_err(|e| e.to_string())?;
            crate::privacy::private_file(&backup).map_err(|e| e.to_string())?;
        }
        persist(&self.path, &settings)?;
        *self.current.write().map_err(|e| e.to_string())? = settings;
        *self.error.lock().map_err(|e| e.to_string())? = None;
        Ok(())
    }

    pub fn trust(&self, path: &str) -> Result<(), String> {
        if self.error.lock().map_err(|e| e.to_string())?.is_some() {
            return Err("Repair settings before trusting a project.".into());
        }
        let path = crate::files::ops::root(path)?
            .to_string_lossy()
            .into_owned();
        let _guard = self.writes.lock().map_err(|e| e.to_string())?;
        let mut settings = self.get();
        if !settings.trusted_projects.contains(&path) {
            settings.trusted_projects.push(path);
        }
        persist(&self.path, &settings)?;
        *self.current.write().map_err(|e| e.to_string())? = settings;
        Ok(())
    }

    pub fn require_trusted(&self, path: &str) -> Result<(), String> {
        let path = crate::files::ops::root(path)?
            .to_string_lossy()
            .into_owned();
        if self.get().trusted_projects.contains(&path) {
            Ok(())
        } else {
            Err("Trust this project before running an agent. Project hooks and configured tools may execute when the CLI starts.".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_keep_paths_full_access_and_existing_history_discovery() {
        let dir = std::env::temp_dir().join(format!("code-ui-legacy-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("settings.json");
        let raw =
            br#"{"projectsRoot":"/custom/projects","binaries":{"codex":"/custom/bin/codex"}}"#;
        fs::write(&path, raw).unwrap();
        let settings = Settings::load(&path).unwrap();
        assert_eq!(settings.projects_root, "/custom/projects");
        assert_eq!(
            settings.binaries["codex"].as_deref(),
            Some("/custom/bin/codex")
        );
        assert_eq!(settings.default_permission, Permission::Yolo);
        assert_eq!(settings.history_agents, AGENTS);
        assert!(settings.setup_complete);
        assert_eq!(fs::read(&path).unwrap(), raw);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_settings_recover_without_overwriting_the_original() {
        let dir = std::env::temp_dir().join(format!("code-ui-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, "{broken").unwrap();
        let store = SettingsStore::load(path.clone());
        assert!(store.snapshot().error.is_some());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{broken");
        assert!(store.trust(dir.to_str().unwrap()).is_err());
        let mut settings = store.get();
        settings.projects_root = dir.join("new projects").to_string_lossy().into_owned();
        assert!(store.save(settings.clone(), false).is_err());
        store.save(settings, true).unwrap();
        let backup = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.invalid-")
            })
            .unwrap();
        assert_eq!(fs::read_to_string(backup).unwrap(), "{broken");
        assert!(store.snapshot().error.is_none());
        assert!(Path::new(&store.get().projects_root).is_dir());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trust_is_explicit_canonical_and_survives_a_preferences_save() {
        let dir = std::env::temp_dir().join(format!("code-ui-trust-{}", uuid::Uuid::new_v4()));
        let store = SettingsStore::load(dir.join("settings.json"));
        let project = dir.join("project");
        fs::create_dir(&project).unwrap();
        let mut stale = store.get();
        stale.projects_root = dir.to_string_lossy().into_owned();
        assert!(store.require_trusted(project.to_str().unwrap()).is_err());
        store.trust(project.to_str().unwrap()).unwrap();
        store.save(stale, false).unwrap();
        store
            .require_trusted(project.join(".").to_str().unwrap())
            .unwrap();
        assert!(
            SettingsStore::load(dir.join("settings.json"))
                .require_trusted(project.to_str().unwrap())
                .is_ok()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn creates_defaults_and_preserves_existing_settings() {
        let dir = std::env::temp_dir().join(format!("code-ui-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        let defaults = Settings::load(&path).unwrap();
        assert_eq!(defaults.binaries.len(), 4);
        let custom = Settings {
            projects_root: "/tmp/projects".into(),
            ..defaults
        };
        fs::write(&path, serde_json::to_vec(&custom).unwrap()).unwrap();
        assert_eq!(
            Settings::load(&path).unwrap().projects_root,
            "/tmp/projects"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn projects_root_defaults_to_projects_in_the_home_folder() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            Path::new(&Settings::default().projects_root),
            home.join("Projects")
        );
    }

    #[test]
    fn a_leading_tilde_means_the_home_folder() {
        let home = Path::new("/home/someone");
        assert_eq!(expand_home("~/Code", home), "/home/someone/Code");
        assert_eq!(expand_home("~", home), "/home/someone");
        assert_eq!(expand_home("/srv/code", home), "/srv/code");
        assert_eq!(expand_home("~other/code", home), "~other/code");
        let dir = std::env::temp_dir().join(format!("code-ui-settings-{}", uuid::Uuid::new_v4()));
        let path = dir.join("settings.json");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, br#"{"projectsRoot":"~/work","binaries":{}}"#).unwrap();
        assert_eq!(
            Path::new(&Settings::load(&path).unwrap().projects_root),
            dirs::home_dir().unwrap().join("work")
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
