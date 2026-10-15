pub mod chats;
use rusqlite::{Connection, OptionalExtension};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Database(pub Mutex<Connection>);

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let on_disk = path != Path::new(":memory:");
        if on_disk && let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            crate::privacy::private_dir(parent)?;
        }
        let mut connection = Connection::open(path)?;
        if on_disk {
            crate::privacy::private_file(path)?;
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut connection)?;
        recover(&mut connection)?;
        Ok(Self(Mutex::new(connection)))
    }

    pub fn get_ui_state(&self, key: &str) -> Result<Option<String>, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row("SELECT value FROM ui_state WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn set_ui_state(&self, key: &str, value: &str) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.execute(
            "INSERT INTO ui_state(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value]).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn migrate(connection: &mut Connection) -> rusqlite::Result<()> {
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > 3 {
        return Err(rusqlite::Error::InvalidParameterName(
            "Database is newer than this app".into(),
        ));
    }
    if version == 0 {
        let tx = connection.transaction()?;
        tx.execute_batch(include_str!("schema.sql"))?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
    }
    if version < 2 {
        let tx = connection.transaction()?;
        tx.execute_batch(
            "ALTER TABLE agent_sessions ADD COLUMN context_used INTEGER;
            ALTER TABLE agent_sessions ADD COLUMN context_window INTEGER;",
        )?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
    }
    if version < 3 {
        let tx = connection.transaction()?;
        let rows = {
            let mut q =
                tx.prepare("SELECT id,payload,answer FROM interactions WHERE answer IS NOT NULL")?;
            q.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, payload, raw) in rows {
            if let Ok(payload) = serde_json::from_str(&payload) {
                let answer = crate::privacy::safe_answer(&payload, Some(raw));
                tx.execute(
                    "UPDATE interactions SET answer=?2 WHERE id=?1",
                    rusqlite::params![id, answer],
                )?;
            }
        }
        tx.pragma_update(None, "user_version", 3)?;
        tx.commit()?;
    }
    Ok(())
}

fn recover(connection: &mut Connection) -> rusqlite::Result<()> {
    let tx = connection.transaction()?;
    tx.execute("INSERT INTO messages(chat_id, seq, role, text, created_at)
        SELECT id, COALESCE((SELECT MAX(seq) FROM messages WHERE chat_id = chats.id), 0) + 1,
        'notice', 'Interrupted when the app closed.', ?1 FROM chats WHERE status IN ('working', 'needs_input')", [now_ms()])?;
    tx.execute(
        "UPDATE steps SET status = 'interrupted', ended_at = ?1 WHERE status = 'running'",
        [now_ms()],
    )?;
    tx.execute(
        "UPDATE chats SET status = 'interrupted' WHERE status IN ('working', 'needs_input')",
        [],
    )?;
    tx.execute(
        "UPDATE interactions SET status = 'expired', resolved_at = ?1 WHERE status = 'pending'",
        [now_ms()],
    )?;
    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_removes_existing_secret_answers_and_snapshot_stays_redacted() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        let payload = serde_json::json!({"questions":[{"id":"q","secret":true}]});
        let answer = serde_json::json!({"type":"answers","answers":[{"questionId":"q","labels":[],"other":"private-token"}],"note":"private-note"});
        c.execute("INSERT INTO chats(id,project_path,title,active_agent,created_at,updated_at) VALUES('chat','/scratch','Title','codex',1,1)", []).unwrap();
        c.execute("INSERT INTO interactions(id,chat_id,turn_id,agent,kind,payload,status,answer,created_at) VALUES('q','chat','turn','codex','question',?1,'answered',?2,1)", rusqlite::params![payload.to_string(),answer.to_string()]).unwrap();
        c.pragma_update(None, "user_version", 2).unwrap();
        migrate(&mut c).unwrap();
        let retained: String = c
            .query_row("SELECT answer FROM interactions", [], |r| r.get(0))
            .unwrap();
        assert!(!retained.contains("private-"));
        let db = Database(Mutex::new(c));
        assert!(
            !db.snapshot("chat").unwrap()["interactions"]
                .to_string()
                .contains("private-")
        );
    }

    #[test]
    fn migration_is_repeatable_and_recovery_keeps_chat_history() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        migrate(&mut c).unwrap();
        c.execute(
            "INSERT INTO chats(id,project_path,title,active_agent,status,created_at,updated_at)
            VALUES ('chat','/tmp','Title','claude','working',1,1)",
            [],
        )
        .unwrap();
        c.execute("INSERT INTO messages(chat_id,seq,role,text,created_at) VALUES ('chat',1,'user','Remember this',1)", []).unwrap();
        c.execute(
            "INSERT INTO interactions(id,chat_id,turn_id,agent,kind,payload,status,created_at)
            VALUES ('q','chat','turn','claude','question','{}','pending',1)",
            [],
        )
        .unwrap();
        recover(&mut c).unwrap();
        recover(&mut c).unwrap();
        let status: String = c
            .query_row("SELECT status FROM chats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "interrupted");
        let count: u32 = c
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let status: String = c
            .query_row("SELECT status FROM interactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "expired");
        c.prepare("SELECT context_used, context_window FROM agent_sessions")
            .unwrap();
        let version: u32 = c
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 3);
        let db = Database(Mutex::new(c));
        db.set_ui_state("tabs", "[]").unwrap();
        db.set_ui_state("tabs", "[1]").unwrap();
        assert_eq!(db.get_ui_state("tabs").unwrap().as_deref(), Some("[1]"));
    }
}
