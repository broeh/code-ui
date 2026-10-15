pub mod handoff;
use crate::{agents::*, db::Database, env::SharedEnvironment};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, mpsc, oneshot};

type EventSink = Arc<dyn Fn(&str, Value) + Send + Sync>;
type AdapterFactory = Arc<
    dyn Fn(StartContext) -> (mpsc::Sender<AgentCommand>, mpsc::Receiver<AgentEvent>) + Send + Sync,
>;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendTarget {
    pub chat_id: Option<String>,
    pub draft: Option<Draft>,
}
#[derive(Deserialize)]
pub struct Draft {
    pub project: String,
    pub agent: AgentKind,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub chat_id: String,
}
pub struct Runtime {
    pub db: Arc<Database>,
    environment: Arc<SharedEnvironment>,
    log_dir: PathBuf,
    sink: EventSink,
    factory: AdapterFactory,
    chats: Mutex<HashMap<String, mpsc::Sender<Command>>>,
}
enum Action {
    Send {
        text: String,
        settings: TurnSettings,
        mode: Option<handoff::HandoffMode>,
    },
    Stop,
    Retry,
    ResumeQueue,
    Respond {
        id: String,
        response: InteractionResponse,
    },
    Unqueue {
        pos: i64,
    },
    Shutdown,
    Close,
}
struct Command {
    action: Action,
    reply: oneshot::Sender<Result<Value, String>>,
}
impl Runtime {
    pub fn new(
        db: Arc<Database>,
        environment: Arc<SharedEnvironment>,
        log_dir: PathBuf,
        sink: EventSink,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            environment,
            log_dir,
            sink,
            factory: Arc::new(spawn_adapter),
            chats: Mutex::new(HashMap::new()),
        })
    }
    fn emit(&self, chat: &str, event: Option<&AgentEvent>) {
        if let Some(event) = event {
            (self.sink)("chat-event", json!({"chatId":chat,"event":event}));
        }
        if event.is_some_and(|event| {
            matches!(
                event,
                AgentEvent::ReplyDelta { .. } | AgentEvent::StepUpdated { .. }
            )
        }) {
            return;
        }
        if let Ok(status) = self.db.status(chat) {
            (self.sink)(
                "chat-status",
                json!({"chatId":chat,"status":status,"unread":false,"needsInput":status=="needs_input"}),
            );
        }
    }
    async fn command(self: &Arc<Self>, chat: &str, action: Action) -> Result<Value, String> {
        let sender = {
            let mut chats = self.chats.lock().await;
            if let Some(sender) = chats.get(chat) {
                sender.clone()
            } else {
                let snapshot = self.db.snapshot(chat)?;
                let project = snapshot["project"]
                    .as_str()
                    .ok_or("Chat has no project")?
                    .into();
                let (sender, commands) = mpsc::channel(32);
                chats.insert(chat.into(), sender.clone());
                let (event_tx, event_rx) = mpsc::channel(256);
                let actor = ChatActor {
                    runtime: self.clone(),
                    chat: chat.into(),
                    project,
                    commands,
                    agents: HashMap::new(),
                    event_tx,
                    events: event_rx,
                    preparing: None,
                    step_output: HashMap::new(),
                    deltas: HashMap::new(),
                    reply_delta: String::new(),
                    busy: false,
                    paused: matches!(
                        snapshot["status"].as_str(),
                        Some("stopped" | "failed" | "interrupted")
                    ),
                    stopping: false,
                    turn: String::new(),
                    settings: TurnSettings::default(),
                };
                tokio::spawn(actor.run());
                sender
            }
        };
        let (reply, rx) = oneshot::channel();
        sender
            .send(Command { action, reply })
            .await
            .map_err(|_| "Chat closed")?;
        rx.await.map_err(|_| "Chat closed")?
    }
    pub async fn send(
        self: &Arc<Self>,
        target: SendTarget,
        text: String,
        settings: TurnSettings,
    ) -> Result<SendResult, String> {
        let value = self.send_with_handoff(target, text, settings, None).await?;
        let chat_id = value["chatId"]
            .as_str()
            .ok_or("Choose how to hand over first.")?
            .into();
        Ok(SendResult { chat_id })
    }
    pub fn preview(&self, chat: &str, target: AgentKind) -> Result<Value, String> {
        let missed = handoff::missed(&self.db.snapshot(chat)?, target);
        Ok(
            json!({"count":missed.len(),"lastFrom":handoff::last_from(&missed),"from":handoff::last_from(&missed)}),
        )
    }
    pub async fn send_with_handoff(
        self: &Arc<Self>,
        target: SendTarget,
        text: String,
        settings: TurnSettings,
        mode: Option<handoff::HandoffMode>,
    ) -> Result<Value, String> {
        if text.trim().is_empty() {
            return Err("Write a message first.".into());
        }
        let chat = if let Some(chat) = target.chat_id {
            let preview = self.preview(&chat, settings.agent)?;
            if preview["count"].as_u64().unwrap_or(0) > 0 && mode.is_none() {
                return Ok(json!({"needsHandoffChoice":preview}));
            }
            chat
        } else {
            let draft = target.draft.ok_or("Choose a project first.")?;
            if draft.agent != settings.agent {
                return Err("The draft and selected agent differ.".into());
            }
            let project = std::fs::canonicalize(&draft.project).map_err(|e| e.to_string())?;
            if !project.is_dir() {
                return Err("The project must be a folder.".into());
            }
            self.db
                .create_chat(&project.to_string_lossy(), &text, &settings)?
        };
        self.command(
            &chat,
            Action::Send {
                text,
                settings,
                mode,
            },
        )
        .await?;
        Ok(json!({"chatId":chat}))
    }
    pub async fn stop(self: &Arc<Self>, chat: &str) -> Result<(), String> {
        self.command(chat, Action::Stop).await.map(|_| ())
    }
    pub async fn retry(self: &Arc<Self>, chat: &str) -> Result<(), String> {
        self.command(chat, Action::Retry).await.map(|_| ())
    }
    pub async fn resume_queue(self: &Arc<Self>, chat: &str) -> Result<(), String> {
        self.command(chat, Action::ResumeQueue).await.map(|_| ())
    }
    pub async fn respond(
        self: &Arc<Self>,
        chat: &str,
        id: String,
        response: InteractionResponse,
    ) -> Result<(), String> {
        self.command(chat, Action::Respond { id, response })
            .await
            .map(|_| ())
    }
    pub async fn unqueue(self: &Arc<Self>, chat: &str, pos: i64) -> Result<String, String> {
        let value = self.command(chat, Action::Unqueue { pos }).await?;
        Ok(value.as_str().unwrap_or_default().into())
    }
    pub async fn close(self: &Arc<Self>, chat: &str) -> Result<(), String> {
        let sender = self.chats.lock().await.remove(chat);
        if let Some(sender) = sender {
            let (reply, rx) = oneshot::channel();
            sender
                .send(Command {
                    action: Action::Close,
                    reply,
                })
                .await
                .map_err(|_| "Chat closed")?;
            rx.await.map_err(|_| "Chat closed")??;
        }
        Ok(())
    }
    pub async fn shutdown(self: &Arc<Self>) {
        let senders: Vec<_> = self
            .chats
            .lock()
            .await
            .drain()
            .map(|(_, sender)| sender)
            .collect();
        let mut waits = Vec::new();
        for sender in senders {
            let (reply, rx) = oneshot::channel();
            let _ = sender
                .send(Command {
                    action: Action::Shutdown,
                    reply,
                })
                .await;
            waits.push(rx);
        }
        for wait in waits {
            let _ = wait.await;
        }
        if let Err(error) = self.db.interrupt_active() {
            tracing::error!(%error,"Failed to mark interrupted chats");
        }
    }
}
struct Connection {
    sender: mpsc::Sender<AgentCommand>,
    reader: tokio::task::JoinHandle<()>,
    used: tokio::time::Instant,
}
struct PendingTurn {
    text: String,
    settings: TurnSettings,
    missed: Vec<handoff::Message>,
}
struct ChatActor {
    runtime: Arc<Runtime>,
    chat: String,
    project: PathBuf,
    commands: mpsc::Receiver<Command>,
    agents: HashMap<AgentKind, Connection>,
    event_tx: mpsc::Sender<(AgentKind, AgentEvent)>,
    events: mpsc::Receiver<(AgentKind, AgentEvent)>,
    busy: bool,
    paused: bool,
    stopping: bool,
    turn: String,
    settings: TurnSettings,
    preparing: Option<PendingTurn>,
    step_output: HashMap<String, String>,
    deltas: HashMap<String, AgentEvent>,
    reply_delta: String,
}
impl ChatActor {
    fn session_settings(&self, snapshot: &Value, kind: AgentKind) -> TurnSettings {
        let session = snapshot["sessions"]
            .as_array()
            .and_then(|rows| rows.iter().find(|s| s["agent"] == kind.as_str()));
        session
            .map(|s| TurnSettings {
                agent: kind,
                model: s["model"].as_str().map(str::to_owned),
                effort: s["effort"].as_str().map(str::to_owned),
                permission: serde_json::from_value(s["permission"].clone())
                    .unwrap_or(Permission::Ask),
            })
            .unwrap_or(TurnSettings {
                agent: kind,
                ..TurnSettings::default()
            })
    }
    fn ensure_agent(&mut self, settings: &TurnSettings) -> Result<(), String> {
        let kind = settings.agent;
        let binary = self
            .runtime
            .environment
            .binary(kind.as_str())
            .ok_or_else(|| {
                format!(
                    "{} is disabled or not installed. Check Settings.",
                    handoff::name(kind)
                )
            })?;
        if self.agents.contains_key(&kind) {
            return Ok(());
        }
        let snapshot = self.runtime.db.snapshot(&self.chat)?;
        let native_id = snapshot["sessions"]
            .as_array()
            .and_then(|rows| rows.iter().find(|s| s["agent"] == kind.as_str()))
            .and_then(|s| s["nativeId"].as_str())
            .map(str::to_owned);
        let (sender, mut events) = (self.runtime.factory)(StartContext {
            cwd: self.project.clone(),
            env: self.runtime.environment.vars(),
            binary,
            native_id,
            chat_id: self.chat.clone(),
            settings: settings.clone(),
            log_dir: self.runtime.log_dir.clone(),
        });
        let tx = self.event_tx.clone();
        let reader = tokio::spawn(async move {
            let mut exited = false;
            while let Some(event) = events.recv().await {
                exited |= matches!(event, AgentEvent::Exited { .. });
                if tx.send((kind, event)).await.is_err() {
                    return;
                }
            }
            if !exited {
                let _ = tx
                    .send((
                        kind,
                        AgentEvent::Exited {
                            code: None,
                            stderr_tail: "Agent connection closed.".into(),
                        },
                    ))
                    .await;
            }
        });
        self.agents.insert(
            kind,
            Connection {
                sender,
                reader,
                used: tokio::time::Instant::now(),
            },
        );
        Ok(())
    }
    async fn reap_idle(&mut self) {
        let expired: Vec<_> = self
            .agents
            .iter()
            .filter(|(kind, connection)| {
                (!self.busy || **kind != self.settings.agent)
                    && connection.used.elapsed() >= std::time::Duration::from_secs(20 * 60)
            })
            .map(|(kind, _)| *kind)
            .collect();
        for kind in expired {
            if let Some(connection) = self.agents.remove(&kind) {
                connection.reader.abort();
                let _ = connection.sender.send(AgentCommand::Shutdown).await;
                tokio::spawn(async move {
                    let _ = connection.reader.await;
                });
            }
        }
    }
    async fn start(
        &mut self,
        text: String,
        settings: TurnSettings,
        mode: Option<handoff::HandoffMode>,
    ) -> Result<(), String> {
        let snapshot = self.runtime.db.snapshot(&self.chat)?;
        let missed = handoff::missed(&snapshot, settings.agent);
        if !missed.is_empty() && mode == Some(handoff::HandoffMode::Summary) {
            if let Some(source) = handoff::last_from(&missed) {
                let summarizer = self.session_settings(&snapshot, source);
                if let Err(message) = self.ensure_agent(&summarizer) {
                    self.runtime.db.notice(&self.chat, &format!("Could not prepare a summary: {message} Handing over all missed messages."))?;
                    return self
                        .start_ready(text, settings, missed, handoff::HandoffMode::All)
                        .await;
                }
                self.runtime
                    .db
                    .enqueue_mode(&self.chat, &text, &settings, mode, true)?;
                self.preparing = Some(PendingTurn {
                    text,
                    settings,
                    missed,
                });
                self.settings = summarizer.clone();
                self.turn = uuid::Uuid::now_v7().to_string();
                self.busy = true;
                self.stopping = false;
                (self.runtime.sink)(
                    "chat-event",
                    json!({"chatId":self.chat,"event":{"type":"handoffStarted"}}),
                );
                self.runtime.emit(&self.chat, None);
                self.agents[&source]
                    .sender
                    .send(AgentCommand::Send {
                        turn_id: self.turn.clone(),
                        text: handoff::SUMMARY_PROMPT.into(),
                        settings: summarizer,
                    })
                    .await
                    .map_err(|_| "Summary agent connection closed")?;
                return Ok(());
            }
            self.runtime.db.notice(
                &self.chat,
                "No previous reply to summarize. Handing over all missed messages.",
            )?;
            return self
                .start_ready(text, settings, missed, handoff::HandoffMode::All)
                .await;
        }
        let mode = mode.unwrap_or(handoff::HandoffMode::All);
        self.start_ready(text, settings, missed, mode).await
    }
    async fn start_ready(
        &mut self,
        text: String,
        settings: TurnSettings,
        context: Vec<handoff::Message>,
        mode: handoff::HandoffMode,
    ) -> Result<(), String> {
        self.ensure_agent(&settings)?;
        self.turn = uuid::Uuid::now_v7().to_string();
        self.settings = settings.clone();
        let composed = if context.is_empty() {
            text.clone()
        } else {
            handoff::build(&self.project.to_string_lossy(), &text, &context, mode)
        };
        let count = if mode == handoff::HandoffMode::Last {
            usize::from(context.iter().any(|m| m.role == "assistant"))
        } else {
            context.len()
        };
        let meta = if context.is_empty() {
            json!({})
        } else {
            json!({"handoff":{"mode":mode,"count":count,"context":composed}})
        };
        self.runtime
            .db
            .begin_turn_meta(&self.chat, &text, &self.turn, &settings, meta)?;
        self.busy = true;
        self.paused = false;
        self.stopping = false;
        self.step_output.clear();
        self.reply_delta.clear();
        self.deltas.clear();
        if self.agents[&settings.agent]
            .sender
            .send(AgentCommand::Send {
                turn_id: self.turn.clone(),
                text: composed,
                settings,
            })
            .await
            .is_err()
        {
            self.busy = false;
            self.agents.remove(&self.settings.agent);
            let event = AgentEvent::TurnFinished {
                outcome: TurnOutcome::Failed {
                    message: "Agent connection closed. Retry to resume.".into(),
                },
                reply: None,
                duration_ms: 0,
            };
            self.runtime
                .db
                .apply_event(&self.chat, &self.turn, &self.settings, &event)?;
            self.runtime.emit(&self.chat, Some(&event));
            return Err("Agent connection closed. Retry to resume.".into());
        }
        self.runtime.emit(&self.chat, None);
        Ok(())
    }
    async fn next(&mut self) -> Result<(), String> {
        if !self.busy
            && let Some((pos, text, settings)) = self.runtime.db.next_queued(&self.chat)?
        {
            let mode = self.runtime.db.queue_mode(&self.chat, pos)?;
            self.start(text, settings, mode).await?;
            if pos != 0 {
                self.runtime.db.unqueue(&self.chat, pos)?;
            }
            self.runtime.emit(&self.chat, None);
        }
        Ok(())
    }
    async fn action(&mut self, action: Action) -> Result<Value, String> {
        match action {
            Action::Send {
                text,
                settings,
                mode,
            } => {
                if self.busy {
                    self.runtime
                        .db
                        .enqueue_mode(&self.chat, &text, &settings, mode, false)?;
                    self.runtime.emit(&self.chat, None);
                } else {
                    self.start(text, settings, mode).await?;
                }
            }
            Action::Stop => {
                if self.busy {
                    self.paused = true;
                    self.stopping = true;
                    self.agents[&self.settings.agent]
                        .sender
                        .send(AgentCommand::Interrupt)
                        .await
                        .map_err(|_| "Agent connection closed")?;
                }
            }
            Action::Retry => {
                if !self.busy {
                    if self
                        .runtime
                        .db
                        .next_queued(&self.chat)?
                        .is_some_and(|(pos, _, _)| pos == 0)
                    {
                        self.paused = false;
                        self.next().await?;
                    } else {
                        let snapshot = self.runtime.db.snapshot(&self.chat)?;
                        let kind = serde_json::from_value(snapshot["activeAgent"].clone())
                            .map_err(|e| e.to_string())?;
                        let settings = self.session_settings(&snapshot, kind);
                        self.start(
                            "Continue where you left off.".into(),
                            settings,
                            Some(handoff::HandoffMode::All),
                        )
                        .await?;
                    }
                }
            }
            Action::ResumeQueue => {
                self.paused = false;
                self.next().await?;
            }
            Action::Respond { id, response } => {
                if !self.busy {
                    return Err("This request expired. Retry the conversation.".into());
                }
                let starts_work = matches!(&response,InteractionResponse::Choice{choice_id,..} if choice_id=="allow")
                    && self.runtime.db.snapshot(&self.chat)?["interactions"]
                        .as_array()
                        .is_some_and(|rows| {
                            rows.iter()
                                .any(|r| r["id"] == id && r["payload"]["kind"] == "plan")
                        });
                self.runtime
                    .db
                    .answer_interaction(&self.chat, &id, &response)?;
                self.agents[&self.settings.agent]
                    .sender
                    .send(AgentCommand::Respond {
                        interaction_id: id,
                        response,
                    })
                    .await
                    .map_err(|_| "Agent connection closed")?;
                if starts_work {
                    self.settings.permission = Permission::Ask;
                    self.runtime.db.0.lock().map_err(|e|e.to_string())?.execute("UPDATE agent_sessions SET permission='ask' WHERE chat_id=?1 AND agent=?2",rusqlite::params![self.chat,self.settings.agent.as_str()]).map_err(|e|e.to_string())?;
                }
                self.runtime.emit(&self.chat, None);
            }
            Action::Unqueue { pos } => {
                let text = self.runtime.db.unqueue(&self.chat, pos)?;
                self.runtime.emit(&self.chat, None);
                return Ok(json!(text));
            }
            Action::Shutdown | Action::Close => {}
        }
        Ok(Value::Null)
    }
    fn flush_deltas(&mut self) {
        if !self.reply_delta.is_empty() {
            let text = std::mem::take(&mut self.reply_delta);
            self.runtime
                .emit(&self.chat, Some(&AgentEvent::ReplyDelta { text }));
        }
        for (_, event) in self.deltas.drain() {
            self.runtime.emit(&self.chat, Some(&event));
        }
    }
    async fn event(&mut self, kind: AgentKind, mut event: AgentEvent) -> Result<(), String> {
        if let AgentEvent::SessionReady { .. } = &event {
            let mut settings = self.settings.clone();
            settings.agent = kind;
            self.runtime
                .db
                .apply_event(&self.chat, &self.turn, &settings, &event)?;
            if kind == self.settings.agent
                && let AgentEvent::SessionReady {
                    model: Some(model), ..
                } = &event
            {
                self.settings.model = Some(model.clone());
            }
            self.runtime.emit(&self.chat, Some(&event));
            return Ok(());
        }
        if let AgentEvent::Exited { stderr_tail, .. } = &event {
            self.agents.remove(&kind);
            if !self.busy || kind != self.settings.agent {
                return Ok(());
            }
            let lower = stderr_tail.to_lowercase();
            let message =
                if lower.contains("auth") || lower.contains("login") || lower.contains("sign in") {
                    format!(
                        "{} is not signed in. Run `{}` in a terminal to log in.",
                        handoff::name(kind),
                        kind.as_str()
                    )
                } else if stderr_tail.is_empty() {
                    format!("{} exited without a reply.", handoff::name(kind))
                } else {
                    cap_output(stderr_tail)
                };
            event = AgentEvent::TurnFinished {
                outcome: TurnOutcome::Failed { message },
                reply: None,
                duration_ms: 0,
            };
        }
        if !self.busy || kind != self.settings.agent {
            return Ok(());
        }
        if self.stopping {
            if let AgentEvent::StepFinished { status, .. } = &mut event {
                *status = StepStatus::Interrupted;
            }
            if matches!(event, AgentEvent::TurnFinished { .. }) {
                event = AgentEvent::TurnFinished {
                    outcome: TurnOutcome::Interrupted,
                    reply: None,
                    duration_ms: 0,
                };
            }
        }
        if let AgentEvent::TurnFinished {
            outcome: TurnOutcome::Failed { message },
            ..
        } = &mut event
        {
            *message = agent_error(kind, message);
        }
        if matches!(event, AgentEvent::TurnFinished { .. })
            && let Some(connection) = self.agents.get_mut(&kind)
        {
            connection.used = tokio::time::Instant::now();
        }
        if self.preparing.is_some() {
            match &event {
                AgentEvent::TurnFinished { outcome, reply, .. } => {
                    let pending = self.preparing.take().unwrap();
                    self.busy = false;
                    if matches!(outcome, TurnOutcome::Interrupted) {
                        self.runtime.db.apply_event(
                            &self.chat,
                            &self.turn,
                            &self.settings,
                            &event,
                        )?;
                        self.runtime.emit(&self.chat, Some(&event));
                        self.paused = true;
                        return Ok(());
                    }
                    let (context, mode) = if matches!(outcome, TurnOutcome::Completed)
                        && reply.as_ref().is_some_and(|s| !s.is_empty())
                    {
                        let note = self.runtime.db.handoff_note(
                            &self.chat,
                            reply.as_deref().unwrap(),
                            &self.settings,
                            &self.turn,
                        )?;
                        (vec![note], handoff::HandoffMode::Summary)
                    } else {
                        self.runtime.db.notice(
                            &self.chat,
                            "Could not prepare a summary. Handing over all missed messages.",
                        )?;
                        (pending.missed, handoff::HandoffMode::All)
                    };
                    self.start_ready(pending.text, pending.settings, context, mode)
                        .await?;
                }
                AgentEvent::NeedsInput { .. }
                | AgentEvent::InteractionClosed { .. }
                | AgentEvent::InteractionRejected { .. } => {
                    self.runtime
                        .db
                        .apply_event(&self.chat, &self.turn, &self.settings, &event)?;
                    self.runtime.emit(&self.chat, Some(&event));
                }
                _ => {}
            }
            return Ok(());
        }
        match &mut event {
            AgentEvent::ReplyDelta { text } => {
                self.reply_delta.push_str(text);
                return Ok(());
            }
            AgentEvent::StepUpdated {
                id,
                title,
                output_append,
            } => {
                let output = self.step_output.entry(id.clone()).or_default();
                if let Some(append) = output_append {
                    *output = cap_output(&format!("{output}{append}"));
                }
                let key = id.clone();
                let entry = self.deltas.entry(key).or_insert(AgentEvent::StepUpdated {
                    id: id.clone(),
                    title: title.clone(),
                    output_append: Some(String::new()),
                });
                if let AgentEvent::StepUpdated {
                    output_append: Some(pending),
                    ..
                } = entry
                    && let Some(append) = output_append
                {
                    *pending = cap_output(&format!("{pending}{append}"));
                }
                return Ok(());
            }
            AgentEvent::StepFinished { id, output, .. } if output.is_none() => {
                *output = self.step_output.remove(id);
            }
            _ => {}
        }
        self.flush_deltas();
        self.runtime
            .db
            .apply_event(&self.chat, &self.turn, &self.settings, &event)?;
        self.runtime.emit(&self.chat, Some(&event));
        if let AgentEvent::TurnFinished { outcome, .. } = &event {
            self.busy = false;
            self.stopping = false;
            self.paused = !matches!(outcome, TurnOutcome::Completed);
            if !self.paused {
                self.next().await?;
            }
        }
        Ok(())
    }
    async fn run(mut self) {
        let mut ticks = tokio::time::interval(std::time::Duration::from_millis(50));
        loop {
            tokio::select! {
                command = self.commands.recv() => {
                    let Some(command) = command else { break; };
                    if matches!(command.action, Action::Shutdown | Action::Close) {
                        let closing = matches!(command.action, Action::Close);
                        let connections: Vec<_> = self.agents.drain().map(|(_, connection)| connection).collect();
                        for connection in &connections {
                            let _ = connection.sender.send(AgentCommand::Shutdown).await;
                        }
                        self.events.close();
                        for connection in connections {
                            let _ = connection.reader.await;
                        }
                        if closing && self.busy {
                            let event = AgentEvent::TurnFinished { outcome: TurnOutcome::Interrupted, reply: None, duration_ms: 0 };
                            let result = self.runtime.db.apply_event(&self.chat, &self.turn, &self.settings, &event);
                            self.runtime.emit(&self.chat, Some(&event));
                            let _ = command.reply.send(result.map(|_| Value::Null));
                        } else {
                            let _ = command.reply.send(Ok(Value::Null));
                        }
                        break;
                    }
                    let result = self.action(command.action).await;
                    let _ = command.reply.send(result);
                },
                event = self.events.recv() => {
                    if let Some((kind, event)) = event
                        && let Err(message) = self.event(kind, event).await {
                        self.runtime.emit(&self.chat, Some(&AgentEvent::Error { message }));
                    }
                },
                _ = ticks.tick() => { self.flush_deltas(); self.reap_idle().await; },
            }
        }
    }
}

fn agent_error(kind: AgentKind, message: &str) -> String {
    let lower = message.to_lowercase();
    if [
        "authentication",
        "not authenticated",
        "not signed in",
        "sign in",
        "sign-in",
        "login",
        "unauthorized",
        "expired token",
    ]
    .iter()
    .any(|text| lower.contains(text))
    {
        format!(
            "{} is not signed in. Run `{}` in a terminal to log in.",
            handoff::name(kind),
            kind.as_str()
        )
    } else {
        message.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::AgentStatus;
    use crate::env::Environment;
    use std::{sync::Mutex as StdMutex, time::Duration};
    async fn wait_for(runtime: &Arc<Runtime>, chat: &str, predicate: impl Fn(&Value) -> bool) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if predicate(&runtime.db.snapshot(chat).unwrap()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn queue_stop_retry_interaction_and_restart() {
        let root = std::env::temp_dir().join(format!("code-ui-runtime-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("app.db");
        let db = Arc::new(Database::open(&path).unwrap());
        let environment: Arc<SharedEnvironment> = Arc::new(
            Environment {
                vars: HashMap::new(),
                agents: vec![AgentStatus {
                    agent: "claude".into(),
                    installed: true,
                    enabled: true,
                    problem: None,
                    path: Some("fake".into()),
                    version: None,
                }],
            }
            .into(),
        );
        let received = Arc::new(StdMutex::new(Vec::new()));
        let log = received.clone();
        let factory: AdapterFactory = Arc::new(move |context| {
            let (tx, mut rx) = mpsc::channel(32);
            let (events, out) = mpsc::channel(64);
            let log = log.clone();
            tokio::spawn(async move {
                let _ = events
                    .send(AgentEvent::SessionReady {
                        native_id: context
                            .native_id
                            .unwrap_or_else(|| format!("native-test-{}", context.chat_id)),
                        model: Some("fake-model".into()),
                    })
                    .await;
                while let Some(command) = rx.recv().await {
                    match command {
                        AgentCommand::Send { turn_id, text, .. } => {
                            log.lock().unwrap().push(text.clone());
                            let _ = events.send(AgentEvent::TurnStarted { turn_id }).await;
                            if text == "long" {
                                let _ = events
                                    .send(AgentEvent::StepStarted {
                                        step: Step {
                                            id: "long-command".into(),
                                            parent_id: None,
                                            kind: StepKind::Command,
                                            title: "Running long command".into(),
                                            input: json!({}),
                                        },
                                    })
                                    .await;
                            }
                            if text == "question" {
                                let _ = events
                                    .send(AgentEvent::NeedsInput {
                                        interaction: Interaction {
                                            id: "q".into(),
                                            kind: "question".into(),
                                            title: "Question".into(),
                                            questions: vec![],
                                            approval: None,
                                            plan: None,
                                            can_dismiss: true,
                                            max_text_length: None,
                                        },
                                    })
                                    .await;
                            } else if text != "long" {
                                tokio::time::sleep(Duration::from_millis(30)).await;
                                let _ = events
                                    .send(AgentEvent::TurnFinished {
                                        outcome: TurnOutcome::Completed,
                                        reply: Some(format!("reply:{text}")),
                                        duration_ms: 30,
                                    })
                                    .await;
                            }
                        }
                        AgentCommand::Respond { .. } => {
                            let _ = events
                                .send(AgentEvent::TurnFinished {
                                    outcome: TurnOutcome::Completed,
                                    reply: Some("answered".into()),
                                    duration_ms: 1,
                                })
                                .await;
                        }
                        AgentCommand::Interrupt => {
                            let _ = events
                                .send(AgentEvent::TurnFinished {
                                    outcome: TurnOutcome::Interrupted,
                                    reply: None,
                                    duration_ms: 1,
                                })
                                .await;
                        }
                        AgentCommand::Shutdown => break,
                    }
                }
            });
            (tx, out)
        });
        let runtime = Arc::new(Runtime {
            db: db.clone(),
            environment,
            log_dir: root.clone(),
            sink: Arc::new(|_, _| {}),
            factory,
            chats: Mutex::new(HashMap::new()),
        });
        let settings = TurnSettings::default();
        let chat = runtime
            .send(
                SendTarget {
                    chat_id: None,
                    draft: Some(Draft {
                        project: root.to_string_lossy().into_owned(),
                        agent: AgentKind::Claude,
                    }),
                },
                "long".into(),
                settings.clone(),
            )
            .await
            .unwrap()
            .chat_id;
        let target = || SendTarget {
            chat_id: Some(chat.clone()),
            draft: None,
        };
        runtime
            .send(target(), "second".into(), settings.clone())
            .await
            .unwrap();
        runtime
            .send(target(), "third".into(), settings.clone())
            .await
            .unwrap();
        assert_eq!(
            runtime.db.snapshot(&chat).unwrap()["queue"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        runtime.stop(&chat).await.unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "stopped").await;
        assert_eq!(*received.lock().unwrap(), vec!["long"]);
        assert_eq!(
            runtime.db.snapshot(&chat).unwrap()["steps"][0]["status"],
            "interrupted"
        );
        runtime.retry(&chat).await.unwrap();
        wait_for(&runtime, &chat, |s| {
            s["status"] == "idle" && s["queue"].as_array().unwrap().is_empty()
        })
        .await;
        assert_eq!(
            *received.lock().unwrap(),
            vec!["long", "Continue where you left off.", "second", "third"]
        );
        runtime
            .send(target(), "question".into(), settings)
            .await
            .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "needs_input").await;
        runtime
            .respond(
                &chat,
                "q".into(),
                InteractionResponse::Answers {
                    answers: vec![],
                    note: None,
                },
            )
            .await
            .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        let before = runtime.db.snapshot(&chat).unwrap();
        assert_eq!(before["interactions"][0]["status"], "answered");
        assert_eq!(
            before["sessions"][0]["nativeId"],
            format!("native-test-{chat}")
        );
        let closing_chat = runtime
            .send(
                SendTarget {
                    chat_id: None,
                    draft: Some(Draft {
                        project: root.to_string_lossy().into_owned(),
                        agent: AgentKind::Claude,
                    }),
                },
                "long".into(),
                TurnSettings::default(),
            )
            .await
            .unwrap()
            .chat_id;
        runtime.close(&closing_chat).await.unwrap();
        assert_eq!(
            runtime.db.snapshot(&closing_chat).unwrap()["status"],
            "stopped"
        );
        runtime.shutdown().await;
        drop(runtime);
        drop(db);
        let reopened = Database::open(&path).unwrap().snapshot(&chat).unwrap();
        assert_eq!(before["messages"], reopened["messages"]);
        assert_eq!(before["sessions"], reopened["sessions"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn handoff_sessions_summary_fallback_and_pending_restart() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::env::temp_dir().join(format!("code-ui-handoff-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Arc::new(Database::open(&root.join("app.db")).unwrap());
        let received = Arc::new(StdMutex::new(Vec::new()));
        let starts = Arc::new(StdMutex::new(Vec::new()));
        let summary_behavior = Arc::new(AtomicUsize::new(0));
        let factory: AdapterFactory = {
            let received = received.clone();
            let starts = starts.clone();
            let behavior = summary_behavior.clone();
            Arc::new(move |context| {
                let kind = context.settings.agent;
                starts
                    .lock()
                    .unwrap()
                    .push((kind, context.native_id.clone()));
                let (tx, mut rx) = mpsc::channel(32);
                let (events, out) = mpsc::channel(64);
                let received = received.clone();
                let behavior = behavior.clone();
                tokio::spawn(async move {
                    events
                        .send(AgentEvent::SessionReady {
                            native_id: context
                                .native_id
                                .unwrap_or_else(|| format!("native-{}", kind.as_str())),
                            model: Some("fake".into()),
                        })
                        .await
                        .unwrap();
                    while let Some(command) = rx.recv().await {
                        match command {
                            AgentCommand::Send { turn_id, text, .. } => {
                                received.lock().unwrap().push((kind, text.clone()));
                                events
                                    .send(AgentEvent::TurnStarted { turn_id })
                                    .await
                                    .unwrap();
                                tokio::time::sleep(Duration::from_millis(15)).await;
                                let summary = text == handoff::SUMMARY_PROMPT;
                                if summary {
                                    events
                                        .send(AgentEvent::StepStarted {
                                            step: Step {
                                                id: "hidden".into(),
                                                parent_id: None,
                                                kind: StepKind::Thinking,
                                                title: "Hidden summary step".into(),
                                                input: json!({}),
                                            },
                                        })
                                        .await
                                        .unwrap();
                                }
                                let behavior = behavior.load(Ordering::SeqCst);
                                if text == "long" || (summary && behavior == 2) {
                                    continue;
                                }
                                let outcome = if summary && behavior == 1 {
                                    TurnOutcome::Failed {
                                        message: "Summary failed".into(),
                                    }
                                } else {
                                    TurnOutcome::Completed
                                };
                                events
                                    .send(AgentEvent::TurnFinished {
                                        outcome,
                                        reply: Some(if summary {
                                            "Goal: build. Next: verify.".into()
                                        } else {
                                            format!("{}:reply", kind.as_str())
                                        }),
                                        duration_ms: 15,
                                    })
                                    .await
                                    .unwrap();
                            }
                            AgentCommand::Interrupt => {
                                events
                                    .send(AgentEvent::TurnFinished {
                                        outcome: TurnOutcome::Interrupted,
                                        reply: None,
                                        duration_ms: 1,
                                    })
                                    .await
                                    .unwrap();
                            }
                            AgentCommand::Shutdown => break,
                            _ => {}
                        }
                    }
                });
                (tx, out)
            })
        };
        let environment: Arc<SharedEnvironment> = Arc::new(
            Environment {
                vars: HashMap::new(),
                agents: ["claude", "codex"]
                    .map(|agent| AgentStatus {
                        agent: agent.into(),
                        installed: true,
                        enabled: true,
                        problem: None,
                        path: Some("fake".into()),
                        version: None,
                    })
                    .into(),
            }
            .into(),
        );
        let make_runtime = || {
            Arc::new(Runtime {
                db: db.clone(),
                environment: environment.clone(),
                log_dir: root.clone(),
                sink: Arc::new(|_, _| {}),
                factory: factory.clone(),
                chats: Mutex::new(HashMap::new()),
            })
        };
        let runtime = make_runtime();
        let claude = TurnSettings::default();
        let codex = TurnSettings {
            agent: AgentKind::Codex,
            ..claude.clone()
        };
        let chat = runtime
            .send(
                SendTarget {
                    chat_id: None,
                    draft: Some(Draft {
                        project: root.to_string_lossy().into(),
                        agent: AgentKind::Claude,
                    }),
                },
                "initial".into(),
                claude.clone(),
            )
            .await
            .unwrap()
            .chat_id;
        let target = || SendTarget {
            chat_id: Some(chat.clone()),
            draft: None,
        };
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        let send = |text: &str, settings: TurnSettings, mode| {
            runtime.send_with_handoff(target(), text.into(), settings, Some(mode))
        };
        send("take over", codex.clone(), handoff::HandoffMode::All)
            .await
            .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        assert!(received.lock().unwrap()[1].1.contains("initial"));
        assert!(received.lock().unwrap()[1].1.contains("claude:reply"));
        let missed = handoff::missed(&db.snapshot(&chat).unwrap(), AgentKind::Claude);
        assert_eq!(missed.len(), 2);
        send("return", claude.clone(), handoff::HandoffMode::Last)
            .await
            .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        let return_prompt = received.lock().unwrap()[2].1.clone();
        assert!(return_prompt.contains("codex:reply"));
        assert!(!return_prompt.contains("take over"));
        assert_eq!(starts.lock().unwrap().len(), 2);
        send(
            "summary target",
            codex.clone(),
            handoff::HandoffMode::Summary,
        )
        .await
        .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        let snapshot = db.snapshot(&chat).unwrap();
        assert!(snapshot["steps"].as_array().unwrap().is_empty());
        assert_eq!(
            snapshot["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|m| m["role"] == "handoff")
                .count(),
            1
        );
        assert!(
            received
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .1
                .contains("Goal: build")
        );
        assert!(
            !received
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .1
                .contains("initial")
        );
        summary_behavior.store(1, Ordering::SeqCst);
        send("fallback", claude.clone(), handoff::HandoffMode::Summary)
            .await
            .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        assert!(
            received
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .1
                .contains("conversation so far")
        );
        assert!(
            db.snapshot(&chat).unwrap()["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["text"]
                    .as_str()
                    .is_some_and(|text| text.starts_with("Could not prepare a summary")))
        );
        summary_behavior.store(2, Ordering::SeqCst);
        send(
            "survives restart",
            codex.clone(),
            handoff::HandoffMode::Summary,
        )
        .await
        .unwrap();
        runtime.stop(&chat).await.unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "stopped").await;
        assert_eq!(
            db.snapshot(&chat).unwrap()["queue"][0]["text"],
            "survives restart"
        );
        runtime.shutdown().await;
        let restarted = make_runtime();
        summary_behavior.store(0, Ordering::SeqCst);
        restarted.retry(&chat).await.unwrap();
        wait_for(&restarted, &chat, |s| s["status"] == "idle").await;
        assert!(
            db.snapshot(&chat).unwrap()["queue"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            received
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .1
                .ends_with("survives restart")
        );
        assert!(
            starts
                .lock()
                .unwrap()
                .iter()
                .skip(2)
                .all(|(_, native)| native.is_some())
        );
        restarted.shutdown().await;
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn authentication_errors_include_the_native_login_command() {
        assert_eq!(
            agent_error(AgentKind::Muse, "Authentication failed"),
            "Muse is not signed in. Run `muse` in a terminal to log in."
        );
        assert_eq!(
            agent_error(AgentKind::Codex, "The model does not exist"),
            "The model does not exist"
        );
    }
    #[tokio::test(start_paused = true)]
    async fn idle_reaper_closes_adapter_and_next_send_resumes_native_id() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::env::temp_dir().join(format!("code-ui-idle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let db = Arc::new(Database::open(&root.join("app.db")).unwrap());
        let starts = Arc::new(StdMutex::new(Vec::new()));
        let stops = Arc::new(AtomicUsize::new(0));
        let factory: AdapterFactory = {
            let starts = starts.clone();
            let stops = stops.clone();
            Arc::new(move |context| {
                starts.lock().unwrap().push(context.native_id.clone());
                let stops = stops.clone();
                let (tx, mut rx) = mpsc::channel(32);
                let (events, out) = mpsc::channel(64);
                tokio::spawn(async move {
                    events
                        .send(AgentEvent::SessionReady {
                            native_id: context.native_id.unwrap_or_else(|| "idle-native".into()),
                            model: Some("fake".into()),
                        })
                        .await
                        .unwrap();
                    while let Some(command) = rx.recv().await {
                        match command {
                            AgentCommand::Send { turn_id, .. } => {
                                events
                                    .send(AgentEvent::TurnStarted { turn_id })
                                    .await
                                    .unwrap();
                                events
                                    .send(AgentEvent::TurnFinished {
                                        outcome: TurnOutcome::Completed,
                                        reply: Some("Ready".into()),
                                        duration_ms: 1,
                                    })
                                    .await
                                    .unwrap();
                            }
                            AgentCommand::Shutdown => {
                                stops.fetch_add(1, Ordering::SeqCst);
                                break;
                            }
                            _ => {}
                        }
                    }
                });
                (tx, out)
            })
        };
        let runtime = Arc::new(Runtime {
            db: db.clone(),
            environment: Arc::new(
                Environment {
                    vars: HashMap::new(),
                    agents: vec![AgentStatus {
                        agent: "claude".into(),
                        installed: true,
                        enabled: true,
                        problem: None,
                        path: Some("fake".into()),
                        version: None,
                    }],
                }
                .into(),
            ),
            log_dir: root.clone(),
            sink: Arc::new(|_, _| {}),
            factory,
            chats: Mutex::new(HashMap::new()),
        });
        let settings = TurnSettings::default();
        let chat = runtime
            .send(
                SendTarget {
                    chat_id: None,
                    draft: Some(Draft {
                        project: root.to_string_lossy().into(),
                        agent: AgentKind::Claude,
                    }),
                },
                "first".into(),
                settings.clone(),
            )
            .await
            .unwrap()
            .chat_id;
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        tokio::time::advance(Duration::from_secs(20 * 60 + 1)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(stops.load(Ordering::SeqCst), 1);
        assert_eq!(
            db.snapshot(&chat).unwrap()["sessions"][0]["nativeId"],
            "idle-native"
        );
        runtime
            .send(
                SendTarget {
                    chat_id: Some(chat.clone()),
                    draft: None,
                },
                "second".into(),
                settings,
            )
            .await
            .unwrap();
        wait_for(&runtime, &chat, |s| s["status"] == "idle").await;
        assert_eq!(
            *starts.lock().unwrap(),
            vec![None, Some("idle-native".into())]
        );
        runtime.shutdown().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
