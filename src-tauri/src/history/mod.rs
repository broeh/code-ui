pub mod official;
pub mod readers;
use crate::{
    agents::{AgentKind, TurnSettings},
    db::Database,
};
use readers::{Session, Source};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

type EventSink = Arc<dyn Fn(&str, Value) + Send + Sync>;
pub struct History {
    db: Arc<Database>,
    home: PathBuf,
    environment: Arc<crate::env::SharedEnvironment>,
    settings: Arc<crate::settings::SettingsStore>,
    gate: tokio::sync::Mutex<()>,
    scheduled: Mutex<HashSet<String>>,
    scanned: Mutex<HashMap<String, Instant>>,
    sink: EventSink,
}
impl History {
    pub fn new(
        db: Arc<Database>,
        home: PathBuf,
        environment: Arc<crate::env::SharedEnvironment>,
        settings: Arc<crate::settings::SettingsStore>,
        sink: EventSink,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            home,
            environment,
            settings,
            gate: tokio::sync::Mutex::new(()),
            scheduled: Mutex::new(HashSet::new()),
            scanned: Mutex::new(HashMap::new()),
            sink,
        })
    }
    pub fn request_scan(self: &Arc<Self>, project: Option<String>) {
        if self.settings.get().history_agents.is_empty() {
            return;
        }
        let key = project.unwrap_or_else(|| "*".into());
        if !self.scheduled.lock().unwrap().insert(key.clone()) {
            return;
        }
        let history = self.clone();
        tauri::async_runtime::spawn(async move {
            let delay = history
                .scanned
                .lock()
                .unwrap()
                .get(&key)
                .map(|last| Duration::from_secs(30).saturating_sub(last.elapsed()))
                .unwrap_or_default();
            tokio::time::sleep(delay).await;
            let _guard = history.gate.lock().await;
            history
                .scanned
                .lock()
                .unwrap()
                .insert(key.clone(), Instant::now());
            let db = history.db.clone();
            let home = history.home.clone();
            let agents = history.settings.get().history_agents;
            let vars = history.environment.vars();
            let result =
                tokio::task::spawn_blocking(move || scan_selected(&db, &home, &vars, &agents))
                    .await;
            if let Ok(Err(message)) = result {
                tracing::warn!(%message,"Native session scan failed");
            }
            history.scheduled.lock().unwrap().remove(&key);
            (history.sink)(
                "native-sessions-updated",
                json!({"project":if key=="*"{None}else{Some(key)}}),
            );
        });
    }
    pub async fn open(&self, agent: AgentKind, native: &str) -> Result<String, String> {
        if let Some(chat) = self.db.linked_chat(agent, native)? {
            return Ok(chat);
        }
        if !self.settings.get().history_agents.contains(&agent) {
            return Err("History discovery is disabled for this agent. Check Settings.".into());
        }
        let source = self.db.native_source(agent, native)?;
        let parsed_source = source.clone();
        let parsed = tokio::task::spawn_blocking(move || readers::parse(&parsed_source))
            .await
            .map_err(|e| e.to_string())?;
        let session = match parsed {
            Ok(session) => session,
            Err(error) if matches!(agent, AgentKind::Codex | AgentKind::Muse) => {
                self.settings.require_trusted(
                    source
                        .cwd
                        .as_deref()
                        .ok_or("This conversation has no project folder.")?,
                )?;
                let binary = self.environment.binary(agent.as_str()).ok_or(error)?;
                official::read(source, binary, self.environment.vars()).await?
            }
            Err(error) => return Err(error),
        };
        self.db.import_native(&session)
    }
}
#[cfg(test)]
pub fn scan(db: &Database, home: &Path) -> Result<(), String> {
    scan_selected(db, home, &HashMap::new(), &crate::settings::AGENTS)
}

fn scan_selected(
    db: &Database,
    home: &Path,
    vars: &HashMap<String, String>,
    agents: &[AgentKind],
) -> Result<(), String> {
    let cached = db.native_stamps()?;
    let sources = readers::discover(home, vars, agents);
    let mut rows = Vec::new();
    for source in sources {
        let key = (source.agent, source.native_id.clone());
        if cached.get(&key) == Some(&source.stamp) {
            continue;
        }
        if let (Some(cwd), Some(title)) = (&source.cwd, &source.title)
            && !cwd.is_empty()
            && !title.trim().is_empty()
        {
            rows.push((
                source.clone(),
                cwd.clone(),
                title
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(120)
                    .collect::<String>(),
                source.updated_at,
            ));
            continue;
        }
        match readers::parse(&source) {
            Ok(session) => rows.push((
                session.source,
                session.cwd,
                session.title,
                session.updated_at,
            )),
            Err(_) => tracing::debug!(
                agent = source.agent.as_str(),
                "Skipped unreadable native session"
            ),
        }
    }
    db.cache_native(&rows)
}
impl Database {
    fn native_stamps(&self) -> Result<HashMap<(AgentKind, String), String>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut q = c
            .prepare("SELECT agent,native_id,stamp FROM native_sessions")
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let mut out = HashMap::new();
        for (agent, id, stamp) in rows.flatten() {
            if let Ok(agent) = serde_json::from_value::<AgentKind>(json!(agent)) {
                out.insert((agent, id), stamp);
            }
        }
        Ok(out)
    }
    fn cache_native(&self, rows: &[(Source, String, String, i64)]) -> Result<(), String> {
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        for (source, cwd, title, time) in rows {
            tx.execute("INSERT INTO native_sessions(agent,native_id,cwd,title,updated_at,source_path,stamp) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(agent,native_id) DO UPDATE SET cwd=excluded.cwd,title=excluded.title,updated_at=excluded.updated_at,source_path=excluded.source_path,stamp=excluded.stamp",rusqlite::params![source.agent.as_str(),source.native_id,cwd,title,time,source.path.to_string_lossy(),source.stamp]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    fn native_source(&self, agent: AgentKind, native: &str) -> Result<Source, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        c.query_row("SELECT source_path,stamp,cwd,title,updated_at FROM native_sessions WHERE agent=?1 AND native_id=?2",rusqlite::params![agent.as_str(),native],|r|Ok(Source{agent,native_id:native.into(),path:PathBuf::from(r.get::<_,String>(0)?),stamp:r.get(1)?,cwd:Some(r.get(2)?),title:Some(r.get(3)?),updated_at:r.get(4)?,model:None})).map_err(|e|e.to_string())
    }
    pub fn linked_chat(&self, agent: AgentKind, native: &str) -> Result<Option<String>, String> {
        use rusqlite::OptionalExtension;
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row(
                "SELECT chat_id FROM agent_sessions WHERE agent=?1 AND native_id=?2",
                rusqlite::params![agent.as_str(), native],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn list_native(&self, project: Option<&str>) -> Result<Vec<Value>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut q=c.prepare("SELECT n.agent,n.native_id,n.cwd,n.title,n.updated_at FROM native_sessions n WHERE (?1 IS NULL OR n.cwd=?1 OR substr(n.cwd,1,length(?1)+1)=?1||'/') AND NOT EXISTS(SELECT 1 FROM agent_sessions a WHERE a.agent=n.agent AND a.native_id=n.native_id) ORDER BY n.updated_at DESC").map_err(|e|e.to_string())?;
        q.query_map([project],|r|Ok(json!({"kind":"terminal","agent":r.get::<_,String>(0)?,"id":format!("native:{}:{}",r.get::<_,String>(0)?,r.get::<_,String>(1)?),"nativeId":r.get::<_,String>(1)?,"project":r.get::<_,String>(2)?,"title":r.get::<_,String>(3)?,"updatedAt":r.get::<_,i64>(4)?,"status":"idle","unread":false}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }
    pub fn import_native(&self, session: &Session) -> Result<String, String> {
        use rusqlite::{OptionalExtension, params};
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        if let Some(chat) = tx
            .query_row(
                "SELECT chat_id FROM agent_sessions WHERE agent=?1 AND native_id=?2",
                params![session.source.agent.as_str(), session.source.native_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            return Ok(chat);
        }
        let id = uuid::Uuid::now_v7().to_string();
        tx.execute("INSERT INTO chats(id,project_path,title,active_agent,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6)",params![id,session.cwd,session.title,session.source.agent.as_str(),session.created_at,session.updated_at]).map_err(|e|e.to_string())?;
        let settings: &TurnSettings = &session.settings;
        tx.execute("INSERT INTO agent_sessions(chat_id,agent,native_id,synced_seq,model,effort,permission) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,settings.agent.as_str(),session.source.native_id,session.messages.len() as i64,settings.model,settings.effort,serde_json::to_value(settings.permission).unwrap().as_str()]).map_err(|e|e.to_string())?;
        let mut turn = String::new();
        for (index, message) in session.messages.iter().enumerate() {
            if message.role == "user" || turn.is_empty() {
                turn = uuid::Uuid::now_v7().to_string();
            }
            tx.execute("INSERT INTO messages(chat_id,seq,role,agent,turn_id,text,meta,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![id,index as i64+1,message.role,settings.agent.as_str(),turn,message.text,json!({"imported":true,"model":settings.model}).to_string(),message.created_at]).map_err(|e|e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    #[test]
    fn custom_cli_homes_and_history_opt_out_are_respected() {
        let home = fixture();
        seed(&home);
        let claude = home.join("custom-claude");
        let codex = home.join("custom-codex");
        std::fs::rename(home.join(".claude"), &claude).unwrap();
        std::fs::rename(home.join(".codex"), &codex).unwrap();
        let vars = HashMap::from([
            (
                "CLAUDE_CONFIG_DIR".into(),
                claude.to_string_lossy().into_owned(),
            ),
            ("CODEX_HOME".into(), codex.to_string_lossy().into_owned()),
        ]);
        assert!(readers::discover(&home, &vars, &[]).is_empty());
        for agent in [AgentKind::Claude, AgentKind::Codex] {
            let sources = readers::discover(&home, &vars, &[agent]);
            assert!(!sources.is_empty());
            assert!(sources.iter().all(|source| source.agent == agent));
            assert!(sources.iter().all(|source| source.path.starts_with(
                if agent == AgentKind::Claude {
                    &claude
                } else {
                    &codex
                }
            )));
        }
        std::fs::remove_dir_all(home).unwrap();
    }
    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("code-ui-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn write(path: &Path, rows: &[Value]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            rows.iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
                + "\n",
        )
        .unwrap();
    }
    fn seed(home: &Path) {
        write(
            &home.join(".claude/projects/project/claude-id.jsonl"),
            &[
                json!({"cwd":"/scratch/project","type":"user","timestamp":"2026-01-02T03:04:05Z","message":{"content":"Remember cedar-marker."}}),
                json!({"type":"user","isMeta":true,"message":{"content":"Hidden system instructions"}}),
                json!({"type":"assistant","timestamp":"2026-01-02T03:04:06Z","message":{"model":"haiku","content":[{"type":"thinking","thinking":"private"},{"type":"text","text":"Remembered cedar-marker."},{"type":"tool_use","input":{}}]}}),
            ],
        );
        let id = "11111111-1111-4111-8111-111111111111";
        write(
            &home.join(format!(
                ".codex/sessions/2026/01/02/rollout-2026-01-02T03-04-05-{id}.jsonl"
            )),
            &[
                json!({"type":"session_meta","timestamp":"2026-01-02T03:04:05Z","payload":{"id":id,"cwd":"/scratch/project/sub"}}),
                json!({"type":"turn_context","payload":{"model":"gpt-test","effort":"low","approval_policy":"never"}}),
                json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions <INSTRUCTIONS>injected</INSTRUCTIONS>"}]}}),
                json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"Remember pine-marker."}]}}),
                json!({"type":"response_item","timestamp":"2026-01-02T03:04:07Z","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Remembered pine-marker."}]}}),
                json!({"type":"response_item","payload":{"type":"function_call_output","output":"do not import"}}),
            ],
        );
        write(
            &home.join(".codex/session_index.jsonl"),
            &[json!({"id":id,"thread_name":"Named Codex chat"})],
        );
        let log = home.join(".local/share/muse/sessions/muse-id/session.jsonl");
        write(
            &log,
            &[
                json!({"recorded_at":"2026-01-02T03:04:05Z","children":[{"record_json":json!({"recorded_at":"2026-01-02T03:04:05Z","payload":{"kind":"run","event":{"kind":"started","prompt":"Remember moss-marker."}}}).to_string()}]}),
                json!({"recorded_at":"2026-01-02T03:04:08Z","payload":{"kind":"run","event":{"kind":"assistant_message_committed","text":"Remembered moss-marker."}}}),
            ],
        );
        let index = home.join(".local/share/muse/session-index.db");
        let c = rusqlite::Connection::open(index).unwrap();
        c.execute_batch("CREATE TABLE sessions(session_id TEXT,session_log_path TEXT,workspace_root TEXT,title TEXT,effective_long_title TEXT,first_user_prompt TEXT,updated_at_us INTEGER,msp_last_activity_at_us INTEGER,model_id TEXT,msp_parent_session_id TEXT,prompt_count INTEGER)").unwrap();
        c.execute("INSERT INTO sessions VALUES('muse-id',?1,'/scratch/project','New session',NULL,'Remember moss-marker.',1767323048000000,NULL,'muse-test',NULL,1)",[log.to_string_lossy().as_ref()]).unwrap();
        let base = home.join(".gemini/antigravity-cli");
        std::fs::create_dir_all(base.join("conversations")).unwrap();
        let c = rusqlite::Connection::open(base.join("conversation_summaries.db")).unwrap();
        c.execute_batch("CREATE TABLE conversation_summaries(conversation_id TEXT,title TEXT,preview TEXT,workspace_uris TEXT,last_modified_time TEXT)").unwrap();
        c.execute("INSERT INTO conversation_summaries VALUES('agy-id','Named Agy chat','Preview','[\"file:///scratch/project\"]','2026-01-02 03:04:09+00:00')",[]).unwrap();
        let c = rusqlite::Connection::open(base.join("conversations/agy-id.db")).unwrap();
        c.execute_batch("CREATE TABLE steps(idx INTEGER,step_type INTEGER,step_payload BLOB)")
            .unwrap();
        for (idx, kind, text) in [
            (0, 14, "Remember amber-marker."),
            (1, 101, "Tool output should not import"),
            (2, 15, "Remembered amber-marker."),
        ] {
            let mut blob = vec![10, text.len() as u8];
            blob.extend_from_slice(text.as_bytes());
            c.execute(
                "INSERT INTO steps VALUES(?1,?2,?3)",
                params![idx, kind, blob],
            )
            .unwrap();
        }
    }
    #[test]
    fn four_readers_import_text_without_activity_and_keep_identity() {
        let home = fixture();
        seed(&home);
        let sources = readers::discover(&home, &HashMap::new(), &crate::settings::AGENTS);
        assert_eq!(sources.len(), 4);
        let db = Database::open(Path::new(":memory:")).unwrap();
        scan(&db, &home).unwrap();
        assert_eq!(db.list_native(Some("/scratch/project")).unwrap().len(), 4);
        assert!(db.list_native(Some("/scratch/proj")).unwrap().is_empty());
        for source in sources {
            let session = readers::parse(&source).unwrap();
            assert_eq!(session.messages.len(), 2);
            let chat = db.import_native(&session).unwrap();
            assert_eq!(db.import_native(&session).unwrap(), chat);
            let snapshot = db.snapshot(&chat).unwrap();
            assert_eq!(snapshot["sessions"][0]["nativeId"], source.native_id);
            assert_eq!(snapshot["sessions"][0]["syncedSeq"], 2);
            assert!(snapshot["steps"].as_array().unwrap().is_empty());
            assert!(
                snapshot["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|m| m["meta"]["imported"] == true)
            );
            assert!(crate::runtime::handoff::missed(&snapshot, source.agent).is_empty());
        }
        assert!(db.list_native(None).unwrap().is_empty());
        assert_eq!(
            db.list_app_chats(Some("/scratch/project")).unwrap().len(),
            4
        );
        scan(&db, &home).unwrap();
        assert!(db.list_native(None).unwrap().is_empty());
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn changed_source_stamp_updates_title_and_folder_wildcards_are_literal() {
        let home = fixture();
        seed(&home);
        let db = Database::open(Path::new(":memory:")).unwrap();
        scan(&db, &home).unwrap();
        write(
            &home.join(".codex/session_index.jsonl"),
            &[
                json!({"id":"11111111-1111-4111-8111-111111111111","thread_name":"New native title"}),
            ],
        );
        scan(&db, &home).unwrap();
        assert!(
            db.list_native(None)
                .unwrap()
                .iter()
                .any(|row| row["title"] == "New native title")
        );
        db.0.lock()
            .unwrap()
            .execute("UPDATE native_sessions SET cwd='/tmp/a_b'", [])
            .unwrap();
        assert_eq!(db.list_native(Some("/tmp/a_b")).unwrap().len(), 4);
        assert!(db.list_native(Some("/tmp/a%b")).unwrap().is_empty());
        std::fs::remove_dir_all(home).unwrap();
    }
}
