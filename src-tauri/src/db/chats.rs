use super::{Database, now_ms};
use crate::agents::{AgentEvent, TurnOutcome, TurnSettings, cap_output, oneliner};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

pub fn title(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("New chat")
        .chars()
        .take(60)
        .collect()
}
pub(crate) fn append(
    c: &Connection,
    chat: &str,
    role: &str,
    text: &str,
    turn: Option<&str>,
    settings: Option<&TurnSettings>,
    meta: Value,
) -> rusqlite::Result<i64> {
    let seq: i64 = c.query_row(
        "SELECT COALESCE(MAX(seq),0)+1 FROM messages WHERE chat_id=?1",
        [chat],
        |r| r.get(0),
    )?;
    c.execute("INSERT INTO messages(chat_id,seq,role,agent,turn_id,text,meta,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![chat,seq,role,settings.map(|s|s.agent.as_str()),turn,text,meta.to_string(),now_ms()])?;
    Ok(seq)
}
impl Database {
    pub fn create_chat(
        &self,
        project: &str,
        text: &str,
        settings: &TurnSettings,
    ) -> Result<String, String> {
        let id = uuid::Uuid::now_v7().to_string();
        let c = self.0.lock().map_err(|e| e.to_string())?;
        c.execute("INSERT INTO chats(id,project_path,title,active_agent,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![id,project,title(text),settings.agent.as_str(),now_ms()]).map_err(|e|e.to_string())?;
        Ok(id)
    }
    pub fn status(&self, chat: &str) -> Result<String, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row("SELECT status FROM chats WHERE id=?1", [chat], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())
    }
    pub fn snapshot(&self, chat: &str) -> Result<Value, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut snapshot:Value=c.query_row("SELECT id,project_path,title,status,active_agent,unread FROM chats WHERE id=?1",[chat],|r|Ok(json!({"id":r.get::<_,String>(0)?,"project":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"activeAgent":r.get::<_,String>(4)?,"unread":r.get::<_,bool>(5)?}))).map_err(|e|e.to_string())?;
        macro_rules! rows {
            ($sql:expr,$mapper:expr) => {{
                let mut q = c.prepare($sql).map_err(|e| e.to_string())?;
                let rows = q.query_map([chat], $mapper).map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<Value>, _>>()
                    .map_err(|e| e.to_string())?
            }};
        }
        snapshot["messages"] = json!(rows!(
            "SELECT seq,role,agent,turn_id,text,meta,created_at FROM messages WHERE chat_id=?1 ORDER BY seq",
            |r| Ok(
                json!({"seq":r.get::<_,i64>(0)?,"role":r.get::<_,String>(1)?,"agent":r.get::<_,Option<String>>(2)?,"turnId":r.get::<_,Option<String>>(3)?,"text":r.get::<_,String>(4)?,"meta":serde_json::from_str::<Value>(&r.get::<_,String>(5)?).unwrap_or(json!({})),"createdAt":r.get::<_,i64>(6)?})
            )
        ));
        snapshot["steps"] = json!(rows!(
            "SELECT id,turn_id,kind,CASE WHEN title GLOB 'Ran: *' THEN substr(title,6) ELSE title END,status,detail,parent_id,started_at,ended_at FROM steps WHERE chat_id=?1 ORDER BY rowid",
            |r| Ok(
                json!({"id":r.get::<_,String>(0)?,"turnId":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"title":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"detail":serde_json::from_str::<Value>(&r.get::<_,Option<String>>(5)?.unwrap_or_default()).unwrap_or(json!({})),"parentId":r.get::<_,Option<String>>(6)?,"startedAt":r.get::<_,Option<i64>>(7)?,"endedAt":r.get::<_,Option<i64>>(8)?})
            )
        ));
        snapshot["interactions"] = json!(rows!(
            "SELECT id,turn_id,payload,status,answer FROM interactions WHERE chat_id=?1 ORDER BY created_at",
            |r| {
                let payload =
                    serde_json::from_str::<Value>(&r.get::<_, String>(2)?).unwrap_or(json!({}));
                let answer = crate::privacy::safe_answer(&payload, r.get::<_, Option<String>>(4)?);
                Ok(
                    json!({"id":r.get::<_,String>(0)?,"turnId":r.get::<_,String>(1)?,"payload":payload,"status":r.get::<_,String>(3)?,"answer":answer}),
                )
            }
        ));
        snapshot["queue"] = json!(rows!(
            "SELECT pos,text,settings FROM queue WHERE chat_id=?1 AND (pos<>0 OR (SELECT status FROM chats WHERE id=?1) NOT IN ('working','needs_input')) ORDER BY pos",
            |r| Ok(
                json!({"pos":r.get::<_,i64>(0)?,"text":r.get::<_,String>(1)?,"settings":serde_json::from_str::<Value>(&r.get::<_,String>(2)?).unwrap_or(json!({}))})
            )
        ));
        snapshot["sessions"] = json!(rows!(
            "SELECT agent,native_id,model,effort,permission,synced_seq,context_used,context_window FROM agent_sessions WHERE chat_id=?1",
            |r| Ok(
                json!({"agent":r.get::<_,String>(0)?,"nativeId":r.get::<_,Option<String>>(1)?,"model":r.get::<_,Option<String>>(2)?,"effort":r.get::<_,Option<String>>(3)?,"permission":r.get::<_,String>(4)?,"syncedSeq":r.get::<_,i64>(5)?,"contextUsed":r.get::<_,Option<i64>>(6)?,"contextWindow":r.get::<_,Option<i64>>(7)?})
            )
        ));
        Ok(snapshot)
    }
    pub fn list_app_chats(&self, project: Option<&str>) -> Result<Vec<Value>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut q=c.prepare("SELECT id,project_path,title,status,active_agent,updated_at,unread,EXISTS(SELECT 1 FROM messages m WHERE m.chat_id=chats.id AND json_extract(m.meta,'$.imported')=1) FROM chats WHERE hidden=0 AND (?1 IS NULL OR project_path=?1 OR substr(project_path,1,length(?1)+1)=?1||'/') ORDER BY updated_at DESC").map_err(|e|e.to_string())?;
        q.query_map([project],|r|Ok(json!({"kind":"app","id":r.get::<_,String>(0)?,"project":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"agent":r.get::<_,String>(4)?,"updatedAt":r.get::<_,i64>(5)?,"unread":r.get::<_,bool>(6)?,"imported":r.get::<_,bool>(7)?}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }
    pub fn begin_turn(
        &self,
        chat: &str,
        text: &str,
        turn: &str,
        settings: &TurnSettings,
    ) -> Result<(), String> {
        self.begin_turn_meta(chat, text, turn, settings, json!({}))
    }
    pub fn begin_turn_meta(
        &self,
        chat: &str,
        text: &str,
        turn: &str,
        settings: &TurnSettings,
        meta: Value,
    ) -> Result<(), String> {
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        append(&tx, chat, "user", text, Some(turn), Some(settings), meta)
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM queue WHERE chat_id=?1 AND pos=0", [chat])
            .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO agent_sessions(chat_id,agent,model,effort,permission) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(chat_id,agent) DO UPDATE SET model=COALESCE(excluded.model,agent_sessions.model),effort=excluded.effort,permission=excluded.permission",params![chat,settings.agent.as_str(),settings.model,settings.effort,serde_json::to_value(settings.permission).unwrap().as_str()]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE chats SET status='working',active_agent=?2,updated_at=?3 WHERE id=?1",
            params![chat, settings.agent.as_str(), now_ms()],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn enqueue(&self, chat: &str, text: &str, settings: &TurnSettings) -> Result<(), String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        c.execute("INSERT INTO queue(chat_id,pos,text,settings) SELECT ?1,COALESCE(MAX(pos),0)+1,?2,?3 FROM queue WHERE chat_id=?1",params![chat,text,serde_json::to_string(settings).map_err(|e|e.to_string())?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn next_queued(&self, chat: &str) -> Result<Option<(i64, String, TurnSettings)>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let row = c
            .query_row(
                "SELECT pos,text,settings FROM queue WHERE chat_id=?1 ORDER BY pos LIMIT 1",
                [chat],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        row.map(|(pos, text, settings)| {
            serde_json::from_str(&settings)
                .map(|settings| (pos, text, settings))
                .map_err(|e| e.to_string())
        })
        .transpose()
    }
    pub fn unqueue(&self, chat: &str, pos: i64) -> Result<String, String> {
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        let text = tx
            .query_row(
                "DELETE FROM queue WHERE chat_id=?1 AND pos=?2 RETURNING text",
                params![chat, pos],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(text)
    }
    pub fn apply_event(
        &self,
        chat: &str,
        turn: &str,
        settings: &TurnSettings,
        event: &AgentEvent,
    ) -> Result<(), String> {
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        let id = |native: &str| format!("{turn}:{native}");
        match event {
            AgentEvent::TurnStarted { .. } => {
                tx.execute("UPDATE agent_sessions SET synced_seq=COALESCE((SELECT MAX(seq) FROM messages WHERE chat_id=?1 AND turn_id=?3 AND role='user'),synced_seq) WHERE chat_id=?1 AND agent=?2",params![chat,settings.agent.as_str(),turn]).map_err(|e|e.to_string())?;
            }
            AgentEvent::SessionReady { native_id, model } => {
                tx.execute("UPDATE agent_sessions SET native_id=?3,model=COALESCE(?4,model) WHERE chat_id=?1 AND agent=?2",params![chat,settings.agent.as_str(),native_id,model]).map_err(|e|e.to_string())?;
            }
            AgentEvent::StepStarted { step } => {
                tx.execute("INSERT INTO steps(id,chat_id,turn_id,agent,parent_id,ord,kind,title,status,started_at,detail) VALUES(?1,?2,?3,?4,?5,(SELECT COUNT(*) FROM steps WHERE chat_id=?2 AND turn_id=?3),?6,?7,'running',?8,?9) ON CONFLICT(id) DO UPDATE SET title=excluded.title,detail=excluded.detail",params![id(&step.id),chat,turn,settings.agent.as_str(),step.parent_id.as_deref().map(id),serde_json::to_value(step.kind).unwrap().as_str(),step.title,now_ms(),json!({"input":step.input}).to_string()]).map_err(|e|e.to_string())?;
            }
            AgentEvent::StepFinished {
                id: native,
                status,
                output,
            } => {
                let old: Option<String> = tx
                    .query_row("SELECT title FROM steps WHERE id=?1", [id(native)], |r| {
                        r.get(0)
                    })
                    .optional()
                    .map_err(|e| e.to_string())?;
                tx.execute("UPDATE steps SET status=?2,ended_at=?3,title=?4,detail=json_set(COALESCE(detail,'{}'),'$.output',?5) WHERE id=?1",params![id(native),serde_json::to_value(status).unwrap().as_str(),now_ms(),oneliner::finished(old.as_deref().unwrap_or_default()),output.as_deref().map(cap_output)]).map_err(|e|e.to_string())?;
            }
            AgentEvent::Commentary { id: native, text } => {
                tx.execute("INSERT INTO steps(id,chat_id,turn_id,agent,ord,kind,title,status,started_at,ended_at,detail) VALUES(?1,?2,?3,?4,(SELECT COUNT(*) FROM steps WHERE chat_id=?2 AND turn_id=?3),'other',?5,'done',?6,?6,?7) ON CONFLICT(id) DO UPDATE SET title=excluded.title,detail=excluded.detail",params![id(native),chat,turn,settings.agent.as_str(),text.lines().next().unwrap_or_default().chars().take(100).collect::<String>(),now_ms(),json!({"input":{"commentary":text}}).to_string()]).map_err(|e|e.to_string())?;
            }
            AgentEvent::Usage {
                used_tokens,
                window_tokens,
            } => {
                tx.execute("UPDATE agent_sessions SET context_used=?3,context_window=COALESCE(?4,context_window) WHERE chat_id=?1 AND agent=?2",params![chat,settings.agent.as_str(),*used_tokens as i64,window_tokens.map(|w|w as i64)]).map_err(|e|e.to_string())?;
            }
            AgentEvent::NeedsInput { interaction } => {
                tx.execute("INSERT INTO interactions(id,chat_id,turn_id,agent,kind,payload,status,created_at) VALUES(?1,?2,?3,?4,?5,?6,'pending',?7) ON CONFLICT(id) DO NOTHING",params![interaction.id,chat,turn,settings.agent.as_str(),interaction.kind,serde_json::to_string(interaction).map_err(|e|e.to_string())?,now_ms()]).map_err(|e|e.to_string())?;
                tx.execute("UPDATE chats SET status='needs_input' WHERE id=?1", [chat])
                    .map_err(|e| e.to_string())?;
            }
            AgentEvent::InteractionClosed { id } => {
                tx.execute("UPDATE interactions SET status='expired',resolved_at=?2 WHERE id=?1 AND status='pending'",params![id,now_ms()]).map_err(|e|e.to_string())?;
                tx.execute("UPDATE chats SET status='working' WHERE id=?1 AND status='needs_input' AND NOT EXISTS(SELECT 1 FROM interactions WHERE chat_id=?1 AND status='pending')",[chat]).map_err(|error|error.to_string())?;
            }
            AgentEvent::InteractionRejected { id, message } => {
                let restored = tx.execute("UPDATE interactions SET status='pending',resolved_at=NULL WHERE id=?1 AND chat_id=?2 AND status NOT IN ('expired','pending')",params![id,chat]).map_err(|error|error.to_string())?;
                if restored > 0 {
                    tx.execute("UPDATE chats SET status='needs_input' WHERE id=?1", [chat])
                        .map_err(|error| error.to_string())?;
                }
                append(
                    &tx,
                    chat,
                    "notice",
                    &format!("Could not respond: {message}"),
                    Some(turn),
                    None,
                    json!({}),
                )
                .map_err(|error| error.to_string())?;
            }
            AgentEvent::TurnFinished {
                outcome,
                reply,
                duration_ms,
            } => {
                let status = match outcome {
                    TurnOutcome::Completed => "idle",
                    TurnOutcome::Failed { .. } => "failed",
                    TurnOutcome::Interrupted => "stopped",
                };
                if matches!(outcome, TurnOutcome::Completed) {
                    if let Some(reply) = reply {
                        let seq=append(&tx,chat,"assistant",reply,Some(turn),Some(settings),json!({"model":settings.model,"effort":settings.effort,"durationMs":duration_ms})).map_err(|e|e.to_string())?;
                        tx.execute(
                            "UPDATE agent_sessions SET synced_seq=?3 WHERE chat_id=?1 AND agent=?2",
                            params![chat, settings.agent.as_str(), seq],
                        )
                        .map_err(|e| e.to_string())?;
                        tx.execute("DELETE FROM steps WHERE chat_id=?1 AND turn_id=?2 AND trim(json_extract(detail,'$.input.commentary'))=trim(?3)",params![chat,turn,reply]).map_err(|e|e.to_string())?;
                    }
                } else {
                    let notice = match outcome {
                        TurnOutcome::Failed { message } => format!("Failed: {message}"),
                        _ => "Stopped.".into(),
                    };
                    append(&tx, chat, "notice", &notice, Some(turn), None, json!({}))
                        .map_err(|e| e.to_string())?;
                }
                tx.execute("UPDATE steps SET status=?3,ended_at=?4 WHERE chat_id=?1 AND turn_id=?2 AND status='running'",params![chat,turn,if status=="idle"{"done"}else{"interrupted"},now_ms()]).map_err(|e|e.to_string())?;
                tx.execute("UPDATE interactions SET status='expired',resolved_at=?2 WHERE chat_id=?1 AND status='pending'",params![chat,now_ms()]).map_err(|e|e.to_string())?;
                tx.execute(
                    "UPDATE chats SET status=?2,updated_at=?3 WHERE id=?1",
                    params![chat, status, now_ms()],
                )
                .map_err(|e| e.to_string())?;
            }
            AgentEvent::Error { message } => {
                append(&tx, chat, "notice", message, Some(turn), None, json!({}))
                    .map_err(|e| e.to_string())?;
            }
            _ => {}
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn answer_interaction(
        &self,
        chat: &str,
        id: &str,
        response: &crate::agents::InteractionResponse,
    ) -> Result<(), String> {
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        let payload: String = tx
            .query_row(
                "SELECT payload FROM interactions WHERE id=?1 AND chat_id=?2 AND status='pending'",
                params![id, chat],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or("This request expired. Retry the conversation.")?;
        let payload: Value = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
        let status = match response {
            crate::agents::InteractionResponse::Answers { .. } => "answered",
            crate::agents::InteractionResponse::Choice { choice_id, .. }
                if ["allow", "accept", "acceptForSession"].contains(&choice_id.as_str())
                    || payload["approval"]["choices"]
                        .as_array()
                        .is_some_and(|choices| {
                            choices.iter().any(|choice| {
                                choice["id"] == *choice_id
                                    && matches!(
                                        choice["kind"].as_str(),
                                        Some("allow" | "allowSession")
                                    )
                            })
                        }) =>
            {
                "approved"
            }
            crate::agents::InteractionResponse::Choice { .. } => "declined",
            _ => "dismissed",
        };
        let affected=tx.execute("UPDATE interactions SET status=?3,answer=?4,resolved_at=?5 WHERE id=?1 AND chat_id=?2 AND status='pending'",params![id,chat,status,crate::privacy::redact_answer(&payload, &serde_json::to_value(response).map_err(|e|e.to_string())?).to_string(),now_ms()]).map_err(|e|e.to_string())?;
        if affected == 0 {
            return Err("This request expired. Retry the conversation.".into());
        }
        tx.execute("UPDATE chats SET status='working' WHERE id=?1", [chat])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn interrupt_active(&self) -> Result<(), String> {
        super::recover(&mut *self.0.lock().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

impl Database {
    pub fn rename_chat(&self, chat: &str, title: &str) -> Result<(), String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Enter a chat title.".into());
        }
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE chats SET title=?2,title_locked=1 WHERE id=?1",
                params![chat, title],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn hide_chat(&self, chat: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute("UPDATE chats SET hidden=1 WHERE id=?1", [chat])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn mark_read(&self, chat: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute("UPDATE chats SET unread=0 WHERE id=?1", [chat])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn agent_defaults(&self) -> Result<Vec<TurnSettings>, String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        let mut q = c
            .prepare("SELECT agent,model,effort,permission FROM agent_defaults")
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter()
            .map(|(agent, model, effort, permission)| {
                Ok(TurnSettings {
                    agent: serde_json::from_value(json!(agent)).map_err(|e| e.to_string())?,
                    model,
                    effort,
                    permission: serde_json::from_value(json!(permission))
                        .map_err(|e| e.to_string())?,
                })
            })
            .collect()
    }
    pub fn set_agent_defaults(&self, settings: &TurnSettings) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.execute(
            "INSERT INTO agent_defaults(agent,model,effort,permission) VALUES(?1,?2,?3,?4)
             ON CONFLICT(agent) DO UPDATE SET model=excluded.model,effort=excluded.effort,permission=excluded.permission",
            params![settings.agent.as_str(),settings.model,settings.effort,serde_json::to_value(settings.permission).unwrap().as_str()]
        ).map_err(|e|e.to_string())?;
        Ok(())
    }
}

impl Database {
    pub fn enqueue_mode(
        &self,
        chat: &str,
        text: &str,
        settings: &TurnSettings,
        mode: Option<crate::runtime::handoff::HandoffMode>,
        preparing: bool,
    ) -> Result<(), String> {
        let mut value = serde_json::to_value(settings).map_err(|e| e.to_string())?;
        value["handoffMode"] = serde_json::to_value(mode).map_err(|e| e.to_string())?;
        let c = self.0.lock().map_err(|e| e.to_string())?;
        if preparing {
            c.execute(
                "INSERT OR REPLACE INTO queue(chat_id,pos,text,settings) VALUES(?1,0,?2,?3)",
                params![chat, text, value.to_string()],
            )
            .map_err(|e| e.to_string())?;
            c.execute("UPDATE chats SET status='working' WHERE id=?1", [chat])
                .map_err(|e| e.to_string())?;
        } else {
            c.execute("INSERT INTO queue(chat_id,pos,text,settings) SELECT ?1,COALESCE(MAX(pos),0)+1,?2,?3 FROM queue WHERE chat_id=?1",params![chat,text,value.to_string()]).map_err(|e|e.to_string())?;
        }
        Ok(())
    }
    pub fn queue_mode(
        &self,
        chat: &str,
        pos: i64,
    ) -> Result<Option<crate::runtime::handoff::HandoffMode>, String> {
        let value: String = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row(
                "SELECT settings FROM queue WHERE chat_id=?1 AND pos=?2",
                params![chat, pos],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        serde_json::from_value(
            serde_json::from_str::<Value>(&value).map_err(|e| e.to_string())?["handoffMode"]
                .clone(),
        )
        .map_err(|e| e.to_string())
    }
    pub fn handoff_note(
        &self,
        chat: &str,
        text: &str,
        settings: &TurnSettings,
        turn: &str,
    ) -> Result<crate::runtime::handoff::Message, String> {
        let mut c = self.0.lock().map_err(|e| e.to_string())?;
        let tx = c.transaction().map_err(|e| e.to_string())?;
        let seq = append(
            &tx,
            chat,
            "handoff",
            text,
            Some(turn),
            Some(settings),
            json!({"model":settings.model}),
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE agent_sessions SET synced_seq=?3 WHERE chat_id=?1 AND agent=?2",
            params![chat, settings.agent.as_str(), seq],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(crate::runtime::handoff::Message {
            seq,
            role: "handoff".into(),
            agent: Some(settings.agent),
            text: text.into(),
            meta: json!({"model":settings.model}),
        })
    }
    pub fn notice(&self, chat: &str, text: &str) -> Result<(), String> {
        let c = self.0.lock().map_err(|e| e.to_string())?;
        append(&c, chat, "notice", text, None, None, json!({})).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_hidden_rows_and_agent_defaults_persist() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let settings = TurnSettings {
            model: Some("haiku".into()),
            effort: Some("low".into()),
            ..TurnSettings::default()
        };
        let id = db
            .create_chat("/scratch", "Original title", &settings)
            .unwrap();
        db.rename_chat(&id, "Manual title").unwrap();
        let locked: bool =
            db.0.lock()
                .unwrap()
                .query_row("SELECT title_locked FROM chats WHERE id=?1", [&id], |row| {
                    row.get(0)
                })
                .unwrap();
        assert!(locked);
        db.set_agent_defaults(&settings).unwrap();
        assert_eq!(db.agent_defaults().unwrap()[0], settings);
        db.hide_chat(&id).unwrap();
        assert!(db.list_app_chats(None).unwrap().is_empty());
        assert_eq!(db.snapshot(&id).unwrap()["title"], "Manual title");
    }
    #[test]
    fn finished_commands_show_without_a_ran_prefix() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let settings = TurnSettings::default();
        let chat = db.create_chat("/scratch", "Goal", &settings).unwrap();
        db.begin_turn(&chat, "Goal", "turn", &settings).unwrap();
        db.0.lock()
            .unwrap()
            .execute(
                "INSERT INTO steps(id,chat_id,turn_id,agent,ord,kind,title,status,started_at) VALUES('old',?1,'turn','codex',0,'command','Ran: /usr/bin/bash -lc pwd','done',1)",
                [&chat],
            )
            .unwrap();
        let step = |title: &str| crate::agents::Step {
            id: "new".into(),
            parent_id: None,
            kind: crate::agents::StepKind::Command,
            title: title.into(),
            input: json!({}),
        };
        for event in [
            crate::agents::AgentEvent::StepStarted {
                step: step("Running: cargo test"),
            },
            crate::agents::AgentEvent::StepFinished {
                id: "new".into(),
                status: crate::agents::StepStatus::Done,
                output: None,
            },
        ] {
            db.apply_event(&chat, "turn", &settings, &event).unwrap();
        }
        let steps = db.snapshot(&chat).unwrap()["steps"].clone();
        let titles: Vec<_> = steps
            .as_array()
            .unwrap()
            .iter()
            .map(|step| step["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["/usr/bin/bash -lc pwd", "cargo test"]);
    }
    #[test]
    fn context_usage_persists_per_agent_session() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let settings = TurnSettings::default();
        let chat = db.create_chat("/scratch", "Goal", &settings).unwrap();
        db.begin_turn(&chat, "Goal", "turn", &settings).unwrap();
        let session = |db: &Database| db.snapshot(&chat).unwrap()["sessions"][0].clone();
        assert!(session(&db)["contextUsed"].is_null());
        let usage = |used, window| crate::agents::AgentEvent::Usage {
            used_tokens: used,
            window_tokens: window,
        };
        db.apply_event(&chat, "turn", &settings, &usage(500, Some(1_000)))
            .unwrap();
        db.apply_event(&chat, "turn", &settings, &usage(700, None))
            .unwrap();
        let row = session(&db);
        assert_eq!(row["contextUsed"], 700);
        assert_eq!(row["contextWindow"], 1_000);
    }
    #[test]
    fn native_approval_choice_ids_use_their_presented_kind() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let settings = TurnSettings {
            agent: crate::agents::AgentKind::Muse,
            ..TurnSettings::default()
        };
        let chat = db.create_chat("/scratch", "Goal", &settings).unwrap();
        db.begin_turn(&chat, "Goal", "turn", &settings).unwrap();
        let mut parser = crate::agents::muse::Parser::default();
        parser.begin("turn");
        let event = parser.parse(&json!({"method":"approval/request","params":{"approvalId":"native","sessionId":"session","currentRequirementId":{"approvalId":"native","sourceIndex":0},"availableChoices":[{"choiceId":"custom-provider-allow","decision":"approved","label":"Allow","scope":"once"}]}})).remove(0);
        let crate::agents::AgentEvent::NeedsInput { interaction } = &event else {
            panic!("No card")
        };
        db.apply_event(&chat, "turn", &settings, &event).unwrap();
        db.answer_interaction(
            &chat,
            &interaction.id,
            &crate::agents::InteractionResponse::Choice {
                choice_id: "custom-provider-allow".into(),
                feedback: None,
            },
        )
        .unwrap();
        assert_eq!(
            db.snapshot(&chat).unwrap()["interactions"][0]["status"],
            "approved"
        );
        db.apply_event(
            &chat,
            "turn",
            &settings,
            &crate::agents::AgentEvent::InteractionRejected {
                id: interaction.id.clone(),
                message: "Invalid response".into(),
            },
        )
        .unwrap();
        let snapshot = db.snapshot(&chat).unwrap();
        assert_eq!(snapshot["status"], "needs_input");
        assert_eq!(snapshot["interactions"][0]["status"], "pending");
        assert!(
            snapshot["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| {
                    message["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("Invalid response"))
                })
        );
    }
    #[test]
    fn titles_use_first_line_and_cap_characters() {
        assert_eq!(title("A title\nsecond line"), "A title");
        assert_eq!(title(&"é".repeat(100)).chars().count(), 60);
    }
}
