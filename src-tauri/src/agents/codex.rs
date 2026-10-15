use super::{
    jsonrpc::{Client, Message},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::{sync::mpsc, time::Instant};
#[derive(Default)]
pub struct Parser {
    cwd: String,
    turn: String,
    requests: HashMap<String, Value>,
    phases: HashMap<String, String>,
    reply: String,
    unknown: HashSet<String>,
    plans: usize,
}
fn s(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn id(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
impl Parser {
    pub fn begin(&mut self, turn: &str) {
        self.turn = turn.into();
        self.reply.clear();
        self.phases.clear();
        self.requests.clear();
    }
    pub fn parse(&mut self, msg: &Value) -> Vec<AgentEvent> {
        let method = msg["method"].as_str().unwrap_or_default();
        let params = &msg["params"];
        if msg.get("id").is_some() {
            return self.request(msg);
        }
        match method {
            "item/started" | "item/completed" => {
                let item = &params["item"];
                let key = s(item, "id");
                let kind = s(item, "type");
                let completed = method == "item/completed";
                if kind == "agentMessage" {
                    let phase = s(item, "phase");
                    self.phases.insert(key.clone(), phase.clone());
                    if completed {
                        let text = s(item, "text");
                        if phase == "final_answer" {
                            self.reply = text;
                            Vec::new()
                        } else {
                            vec![AgentEvent::Commentary { id: key, text }]
                        }
                    } else {
                        Vec::new()
                    }
                } else if ["userMessage", "hookPrompt"].contains(&kind.as_str()) {
                    Vec::new()
                } else {
                    let (step_kind, title) = item_title(item, &self.cwd);
                    if completed {
                        let status = if item["status"] == "declined" {
                            StepStatus::Declined
                        } else if item["status"] == "failed"
                            || item["exitCode"].as_i64().is_some_and(|n| n != 0)
                        {
                            StepStatus::Failed
                        } else {
                            StepStatus::Done
                        };
                        let output =
                            item["aggregatedOutput"]
                                .as_str()
                                .map(cap_output)
                                .or_else(|| {
                                    item.get("output").map(|v| {
                                        cap_output(&if let Some(text) = v.as_str() {
                                            text.into()
                                        } else {
                                            v.to_string()
                                        })
                                    })
                                });
                        vec![AgentEvent::StepFinished {
                            id: key,
                            status,
                            output,
                        }]
                    } else {
                        vec![AgentEvent::StepStarted {
                            step: Step {
                                id: key,
                                parent_id: item["parentId"].as_str().map(str::to_owned),
                                kind: step_kind,
                                title,
                                input: item.clone(),
                            },
                        }]
                    }
                }
            }
            "item/agentMessage/delta" => {
                let key = s(params, "itemId");
                let text = s(params, "delta");
                if self.phases.get(&key).is_some_and(|p| p == "final_answer") {
                    self.reply.push_str(&text);
                    vec![AgentEvent::ReplyDelta { text }]
                } else {
                    Vec::new()
                }
            }
            "item/commandExecution/outputDelta" | "item/reasoning/summaryTextDelta" => {
                vec![AgentEvent::StepUpdated {
                    id: s(params, "itemId"),
                    title: None,
                    output_append: Some(s(params, "delta")),
                }]
            }
            "turn/completed" => {
                let turn = &params["turn"];
                if self.reply.is_empty()
                    && let Some(items) = turn["items"].as_array()
                    && let Some(reply) = items.iter().rev().find(|item| {
                        item["type"] == "agentMessage" && item["phase"] == "final_answer"
                    })
                {
                    self.reply = s(reply, "text");
                }
                let outcome = match turn["status"].as_str() {
                    Some("completed") => TurnOutcome::Completed,
                    Some("interrupted") => TurnOutcome::Interrupted,
                    _ => TurnOutcome::Failed {
                        message: turn["error"]["message"]
                            .as_str()
                            .unwrap_or("Codex could not finish this turn.")
                            .into(),
                    },
                };
                vec![AgentEvent::TurnFinished {
                    outcome,
                    reply: (!self.reply.is_empty()).then(|| self.reply.clone()),
                    duration_ms: turn["durationMs"].as_u64().unwrap_or(0),
                }]
            }
            "serverRequest/resolved" => {
                let request_id = id(&params["requestId"]);
                vec![AgentEvent::InteractionClosed {
                    id: format!("{}:codex:{request_id}", self.turn),
                }]
            }
            "error" => vec![AgentEvent::Error {
                message: params["error"]["message"]
                    .as_str()
                    .unwrap_or("Codex reported an error.")
                    .into(),
            }],
            "turn/plan/updated" => {
                self.plans += 1;
                let id = format!("{}:plan:{}", s(params, "turnId"), self.plans);
                let todos: Vec<Value> = params["plan"]
                    .as_array()
                    .map(|steps| {
                        steps
                            .iter()
                            .map(|step| {
                                let status = match step["status"].as_str() {
                                    Some("inProgress") => "in_progress",
                                    Some("completed") => "completed",
                                    _ => "pending",
                                };
                                json!({"text":s(step,"step"),"status":status})
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let output = format!("{} steps", todos.len());
                vec![
                    AgentEvent::StepStarted {
                        step: Step {
                            id: id.clone(),
                            parent_id: None,
                            kind: StepKind::Todo,
                            title: "Updating the task list".into(),
                            input: json!({"todos":todos,"explanation":params["explanation"]}),
                        },
                    },
                    AgentEvent::StepFinished {
                        id,
                        status: StepStatus::Done,
                        output: Some(output),
                    },
                ]
            }
            "thread/tokenUsage/updated" => {
                let usage = &params["tokenUsage"];
                usage["last"]["totalTokens"]
                    .as_u64()
                    .map(|used| {
                        vec![AgentEvent::Usage {
                            used_tokens: used,
                            window_tokens: usage["modelContextWindow"].as_u64(),
                        }]
                    })
                    .unwrap_or_default()
            }
            "turn/started"
            | "thread/status/changed"
            | "thread/started"
            | "account/rateLimits/updated"
            | "account/updated"
            | "mcpServer/startupStatus/updated"
            | "remoteControl/status/changed"
            | "item/reasoning/textDelta"
            | "item/reasoning/summaryPartAdded" => Vec::new(),
            _ => {
                if self.unknown.insert(method.into()) {
                    tracing::warn!(method, "Unknown Codex notification");
                }
                Vec::new()
            }
        }
    }
    fn request(&mut self, msg: &Value) -> Vec<AgentEvent> {
        let method = msg["method"].as_str().unwrap_or_default();
        let params = &msg["params"];
        let key = format!("{}:codex:{}", self.turn, id(&msg["id"]));
        if ![
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
            "item/permissions/requestApproval",
            "item/tool/requestUserInput",
            "execCommandApproval",
            "applyPatchApproval",
        ]
        .contains(&method)
        {
            return Vec::new();
        }
        self.requests.insert(key.clone(), msg.clone());
        let mut interaction = Interaction {
            id: key,
            kind: "approval".into(),
            title: "Allow Codex to take this action?".into(),
            questions: Vec::new(),
            approval: None,
            plan: None,
            can_dismiss: false,
            max_text_length: None,
        };
        if method == "item/tool/requestUserInput" {
            interaction.kind = "question".into();
            interaction.title = "Codex has a question".into();
            if let Some(questions) = params["questions"].as_array() {
                interaction.questions = questions
                    .iter()
                    .map(|q| Question {
                        id: s(q, "id"),
                        header: s(q, "header"),
                        text: s(q, "question"),
                        options: q["options"]
                            .as_array()
                            .map(|options| {
                                options
                                    .iter()
                                    .map(|o| QOption {
                                        label: s(o, "label"),
                                        description: s(o, "description"),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        multi: false,
                        allow_other: q["isOther"] == true || q["options"].is_null(),
                        secret: q["isSecret"] == true,
                    })
                    .collect();
            }
        } else {
            interaction.title =
                if method.contains("fileChange") || method == "applyPatchApproval" {
                    "Allow Codex to edit a file?"
                } else if method.contains("permissions") {
                    "Allow Codex these permissions?"
                } else {
                    "Allow Codex to run a command?"
                }
                .into();
            interaction.approval = Some(Approval {
                summary: params["reason"]
                    .as_str()
                    .unwrap_or("Review this action before continuing.")
                    .into(),
                detail: params["command"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| serde_json::to_string_pretty(params).unwrap_or_default()),
                choices: vec![
                    Choice {
                        id: "accept".into(),
                        label: "Allow".into(),
                        kind: "allow".into(),
                    },
                    Choice {
                        id: "acceptForSession".into(),
                        label: "Allow for this chat".into(),
                        kind: "allowSession".into(),
                    },
                    Choice {
                        id: "decline".into(),
                        label: "Decline".into(),
                        kind: "deny".into(),
                    },
                ],
            });
        }
        vec![AgentEvent::NeedsInput { interaction }]
    }
    pub fn response(&mut self, key: &str, response: &InteractionResponse) -> Result<Value, String> {
        let request = self
            .requests
            .remove(key)
            .ok_or("This request expired. Retry the conversation.")?;
        let method = s(&request, "method");
        let params = &request["params"];
        let result = if method == "item/tool/requestUserInput" {
            let InteractionResponse::Answers { answers, note } = response else {
                return Err("Answer the Codex question to continue.".into());
            };
            let mut map = serde_json::Map::new();
            for answer in answers {
                let mut labels = answer.labels.clone();
                if let Some(other) = answer.other.as_deref().filter(|s| !s.is_empty()) {
                    labels = vec![other.into()];
                }
                if let Some(note) = note.as_deref().filter(|s| !s.is_empty()) {
                    labels.push(format!("Note: {note}"));
                }
                map.insert(answer.question_id.clone(), json!({"answers":labels}));
            }
            json!({"answers":map})
        } else {
            let choice = match response {
                InteractionResponse::Choice { choice_id, .. } => choice_id.as_str(),
                _ => "decline",
            };
            if method == "item/permissions/requestApproval" {
                json!({"permissions":if choice=="decline"{json!({})}else{params["permissions"].clone()},"scope":if choice=="acceptForSession"{"session"}else{"turn"}})
            } else if ["execCommandApproval", "applyPatchApproval"].contains(&method.as_str()) {
                json!({"decision":match choice{"accept"=>json!("approved"),"acceptForSession"=>json!("approved_for_session"),_=>json!({"denied":{"rejection":"The user declined this request."}})}})
            } else {
                json!({"decision":choice})
            }
        };
        Ok(json!({"id":request["id"],"result":result}))
    }
}
fn item_title(item: &Value, cwd: &str) -> (StepKind, String) {
    let kind = s(item, "type");
    let command = s(item, "command");
    let action = &item["commandActions"][0];
    let relative = |path: &str| {
        path.strip_prefix(&format!("{cwd}/"))
            .unwrap_or(path)
            .to_owned()
    };
    let path = relative(action["path"].as_str().unwrap_or("."));
    let (kind, title) = match kind.as_str() {
        "reasoning" => (StepKind::Thinking, "Thinking".into()),
        "commandExecution" => match action["type"].as_str() {
            Some("read") => (StepKind::Read, format!("Reading {path}")),
            Some("listFiles") => (StepKind::List, format!("Listing {path}")),
            Some("search") => (
                StepKind::Search,
                format!(
                    "Searching for {}",
                    action["query"].as_str().unwrap_or_default()
                ),
            ),
            _ => (StepKind::Command, format!("Running: {command}")),
        },
        "fileChange" => (
            StepKind::Edit,
            format!(
                "Editing {}",
                item["changes"]
                    .as_array()
                    .map(|changes| changes
                        .iter()
                        .filter_map(|c| c["path"].as_str())
                        .map(relative)
                        .collect::<Vec<_>>()
                        .join(", "))
                    .unwrap_or_default()
            ),
        ),
        "webSearch" => (StepKind::Web, "Searching the web".into()),
        "mcpToolCall" | "dynamicToolCall" => (
            StepKind::Mcp,
            format!("Using {}", item["tool"].as_str().unwrap_or("a tool")),
        ),
        "collabAgentToolCall" | "subAgentActivity" => {
            (StepKind::Subagent, "Delegating work".into())
        }
        "contextCompaction" => (StepKind::Compact, "Compacting conversation".into()),
        "plan" => (StepKind::Todo, "Updating plan".into()),
        _ => (StepKind::Other, format!("Using {kind}")),
    };
    (kind, title.chars().take(80).collect())
}
fn policy(settings: &TurnSettings, cwd: &str) -> Value {
    match settings.permission {
        Permission::Yolo => {
            json!({"approvalPolicy":"never","approvalsReviewer":"user","sandboxPolicy":{"type":"dangerFullAccess"}})
        }
        _ => {
            json!({"approvalPolicy":"on-request","approvalsReviewer":"user","sandboxPolicy":{"type":"workspaceWrite","writableRoots":[cwd],"networkAccess":false}})
        }
    }
}
async fn initialize(context: &StartContext) -> Result<(Client, String, Option<String>), String> {
    let mut client = Client::spawn(
        context,
        &[
            "app-server".into(),
            "--enable".into(),
            "default_mode_request_user_input".into(),
        ],
    )?;
    client.call("initialize",json!({"clientInfo":{"name":"code_ui","title":"CODE_UI","version":env!("CARGO_PKG_VERSION")}})).await?;
    client.write(&json!({"method":"initialized"})).await?;
    let mut params = json!({"cwd":context.cwd,"approvalPolicy":if context.settings.permission==Permission::Yolo{"never"}else{"on-request"},"approvalsReviewer":"user","sandbox":if context.settings.permission==Permission::Yolo{"danger-full-access"}else{"workspace-write"}});
    if let Some(model) = &context.settings.model {
        params["model"] = json!(model);
    }
    let method = if let Some(native) = &context.native_id {
        params["threadId"] = json!(native);
        "thread/resume"
    } else {
        "thread/start"
    };
    let result = client.call(method, params).await?;
    let native = s(&result["thread"], "id");
    if native.is_empty() {
        return Err("Codex returned no thread id.".into());
    }
    let model = result["model"].as_str().map(str::to_owned);
    Ok((client, native, model))
}
pub async fn run(
    mut context: StartContext,
    mut commands: mpsc::Receiver<AgentCommand>,
    events: mpsc::Sender<AgentEvent>,
) {
    let mut client: Option<Client> = None;
    let mut native = context.native_id.clone().unwrap_or_default();
    let mut native_turn = String::new();
    let mut parser = Parser {
        cwd: context.cwd.to_string_lossy().into_owned(),
        ..Parser::default()
    };
    let mut active = false;
    let mut deadline: Option<Instant> = None;
    loop {
        let stop_at = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(86400));
        tokio::select! {
            command = commands.recv() => match command {
                Some(AgentCommand::Send { turn_id, text, settings }) => {
                    if settings.permission == Permission::Plan {
                        let _ = events.send(AgentEvent::TurnFinished {
                            outcome: TurnOutcome::Failed { message: "Codex does not support Plan permission mode.".into() }, reply: None, duration_ms: 0,
                        }).await;
                        continue;
                    }
                    if active {
                        let _ = events.send(AgentEvent::Error { message: "A Codex turn is already running.".into() }).await;
                        continue;
                    }
                    context.settings = settings.clone();
                    if client.is_none() {
                        match initialize(&context).await {
                            Ok((rpc, id, model)) => {
                                client = Some(rpc); native = id; context.native_id = Some(native.clone());
                                let _ = events.send(AgentEvent::SessionReady { native_id: native.clone(), model }).await;
                            }
                            Err(message) => {
                                let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Failed { message }, reply: None, duration_ms: 0 }).await;
                                continue;
                            }
                        }
                    }
                    parser.begin(&turn_id); active = true;
                    let _ = events.send(AgentEvent::TurnStarted { turn_id }).await;
                    let mut params = policy(&settings, &context.cwd.to_string_lossy());
                    params["threadId"] = json!(native);
                    params["input"] = json!([{"type":"text","text":text}]);
                    if let Some(model) = settings.model { params["model"] = json!(model); }
                    if let Some(effort) = settings.effort { params["effort"] = json!(effort); }
                    match client.as_mut().unwrap().call("turn/start", params).await {
                        Ok(result) => native_turn = s(&result["turn"], "id"),
                        Err(message) => {
                            active = false;
                            let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Failed { message }, reply: None, duration_ms: 0 }).await;
                        }
                    }
                }
                Some(AgentCommand::Respond { interaction_id, response }) => {
                    match parser.response(&interaction_id, &response) {
                        Ok(reply) => {
                            if let Some(client) = &mut client && let Err(message) = client.write(&reply).await {
                                let _ = events.send(AgentEvent::Error { message }).await;
                            }
                        }
                        Err(message) => { let _ = events.send(AgentEvent::Error { message }).await; }
                    }
                }
                Some(AgentCommand::Interrupt) => {
                    if active {
                        if let Some(client) = &mut client {
                            let _ = client.send("turn/interrupt", json!({"threadId":native,"turnId":native_turn})).await;
                        }
                        deadline = Some(Instant::now() + Duration::from_secs(5));
                    }
                }
                Some(AgentCommand::Shutdown) | None => break,
            },
            msg = async { match &mut client { Some(client) => client.next().await, None => std::future::pending().await } } => {
                let Some(msg) = msg else {
                    let tail = client.as_ref().map(|c| c.process.stderr_tail()).unwrap_or_default();
                    client = None;
                    if active { active = false; let _ = events.send(AgentEvent::Exited { code: None, stderr_tail: tail }).await; }
                    continue;
                };
                match client.as_mut().unwrap().router.route(msg.clone()) {
                    Message::Response { result: Err(message), .. } => { let _ = events.send(AgentEvent::Error { message }).await; }
                    Message::Request(_) if msg["method"] == "mcpServer/elicitation/request" => {
                        let _ = client.as_mut().unwrap().write(&json!({"id":msg["id"],"result":{"action":"decline","content":null}})).await;
                        let _ = events.send(AgentEvent::Error { message: "Codex asked for MCP input; not supported yet.".into() }).await;
                    }
                    Message::Request(_) if !["item/commandExecution/requestApproval","item/fileChange/requestApproval","item/permissions/requestApproval","item/tool/requestUserInput","execCommandApproval","applyPatchApproval"].contains(&msg["method"].as_str().unwrap_or_default()) => {
                        let _ = client.as_mut().unwrap().write(&json!({"id":msg["id"],"error":{"code":-32601,"message":"Unsupported client request"}})).await;
                    }
                    Message::Request(_) | Message::Notification(_) if active => {
                        for mut event in parser.parse(&msg) {
                            if matches!(event, AgentEvent::TurnFinished { .. }) {
                                active = false;
                                if deadline.take().is_some() { event = AgentEvent::TurnFinished { outcome: TurnOutcome::Interrupted, reply: None, duration_ms: 0 }; }
                            }
                            if events.send(event).await.is_err() { return; }
                        }
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(stop_at), if deadline.is_some() => {
                if let Some(mut rpc) = client.take() { rpc.shutdown().await; }
                active = false; deadline = None;
                let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Interrupted, reply: None, duration_ms: 0 }).await;
            }
        }
    }
    if let Some(mut client) = client {
        client.shutdown().await;
    }
}
pub async fn models(
    binary: String,
    env: HashMap<String, String>,
    cwd: std::path::PathBuf,
) -> Result<Vec<Value>, String> {
    let context = StartContext {
        binary,
        env,
        cwd,
        native_id: None,
        chat_id: "model-list".into(),
        settings: TurnSettings {
            agent: AgentKind::Codex,
            ..TurnSettings::default()
        },
        log_dir: crate::paths::data_dir()?.join("logs"),
    };
    let mut client = Client::spawn(&context, &["app-server".into()])?;
    let result = async {
        client.call("initialize", json!({"clientInfo":{"name":"code_ui","version":env!("CARGO_PKG_VERSION")}})).await?;
        client.write(&json!({"method":"initialized"})).await?;
        let mut cursor = Value::Null;
        let mut models = Vec::new();
        loop {
            let result = client.call("model/list", json!({"limit":100,"cursor":cursor})).await?;
            if let Some(data) = result["data"].as_array() {
                for model in data {
                    models.push(json!({
                        "id":model["model"], "label":model["displayName"], "isDefault":model["isDefault"],
                        "efforts":model["supportedReasoningEfforts"].as_array().map(|items| items.iter().filter_map(|item|item["reasoningEffort"].as_str()).collect::<Vec<_>>()).unwrap_or_default()
                    }));
                }
            }
            cursor = result["nextCursor"].clone();
            if cursor.is_null() { break; }
        }
        Ok(models)
    }.await;
    client.shutdown().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_usage_notification_reports_the_last_request_and_window() {
        let mut parser = Parser::default();
        parser.begin("app-turn");
        let events = parser.parse(&json!({"method":"thread/tokenUsage/updated","params":{"threadId":"t","turnId":"u","tokenUsage":{"last":{"totalTokens":5200},"total":{"totalTokens":90000},"modelContextWindow":258400}}}));
        assert!(matches!(
            events[..],
            [AgentEvent::Usage {
                used_tokens: 5200,
                window_tokens: Some(258400)
            }]
        ));
    }
    #[test]
    fn plan_updates_become_task_list_steps() {
        let mut parser = Parser::default();
        parser.begin("app-turn");
        let update = json!({"method":"turn/plan/updated","params":{"threadId":"t","turnId":"u","explanation":null,"plan":[
            {"step":"Read the log","status":"completed"},
            {"step":"Fix it","status":"inProgress"},
            {"step":"Test","status":"pending"}]}});
        let events = parser.parse(&update);
        let [
            AgentEvent::StepStarted { step },
            AgentEvent::StepFinished { id, .. },
        ] = events.as_slice()
        else {
            panic!("Expected a started and a finished step")
        };
        assert_eq!(step.kind, StepKind::Todo);
        assert_eq!(&step.id, id);
        assert_eq!(
            step.input["todos"],
            json!([
                {"text":"Read the log","status":"completed"},
                {"text":"Fix it","status":"in_progress"},
                {"text":"Test","status":"pending"}
            ])
        );
        let AgentEvent::StepStarted { step: next } = &parser.parse(&update)[0] else {
            panic!("Expected a step")
        };
        assert_ne!(step.id, next.id);
    }
    fn replay(trace: &str) -> Vec<AgentEvent> {
        let mut parser = Parser::default();
        parser.begin("app-turn");
        trace
            .lines()
            .filter_map(|line| {
                let row: Value = serde_json::from_str(line).unwrap();
                (row["dir"] == "out").then(|| parser.parse(&row["msg"]))
            })
            .flatten()
            .collect()
    }
    #[test]
    fn real_read_fixture_streams_reply_and_steps() {
        let events = replay(include_str!("../../tests/fixtures/codex/read.jsonl"));
        assert!(events.iter().any(
            |event| matches!(event, AgentEvent::StepStarted { step } if step.kind == StepKind::List)
        ));
        assert!(events.iter().any(|event| matches!(event, AgentEvent::StepFinished { output: Some(text), status: StepStatus::Done, .. } if text.contains("protocol test"))));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AgentEvent::ReplyDelta { text } if !text.is_empty()))
        );
        assert!(
            matches!(events.last(), Some(AgentEvent::TurnFinished { outcome: TurnOutcome::Completed, reply: Some(text), .. }) if text.contains("README.md"))
        );
    }
    #[test]
    fn approvals_preserve_native_ids_and_all_three_choices() {
        let trace = include_str!("../../tests/fixtures/codex/approval.jsonl");
        let request = trace
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| {
                row["dir"] == "out"
                    && row["msg"]["method"] == "item/commandExecution/requestApproval"
            })
            .unwrap()["msg"]
            .clone();
        for choice in ["accept", "acceptForSession", "decline"] {
            let mut parser = Parser::default();
            parser.begin("turn");
            let events = parser.parse(&request);
            let AgentEvent::NeedsInput { interaction } = &events[0] else {
                panic!("No card")
            };
            assert!(!interaction.can_dismiss);
            assert_eq!(interaction.approval.as_ref().unwrap().choices.len(), 3);
            let result = parser
                .response(
                    &interaction.id,
                    &InteractionResponse::Choice {
                        choice_id: choice.into(),
                        feedback: None,
                    },
                )
                .unwrap();
            assert_eq!(result["id"], request["id"]);
            assert_eq!(result["result"]["decision"], choice);
        }
    }
    #[test]
    fn real_question_fixture_answers_and_secret_freeform() {
        let trace = include_str!("../../tests/fixtures/codex/question.jsonl");
        let mut request = trace
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["msg"]["method"] == "item/tool/requestUserInput")
            .unwrap()["msg"]
            .clone();
        request["id"] = json!(0);
        let mut parser = Parser::default();
        parser.begin("turn");
        let events = parser.parse(&request);
        let AgentEvent::NeedsInput { interaction } = &events[0] else {
            panic!("No card")
        };
        let response = InteractionResponse::Answers {
            answers: vec![Answer {
                question_id: interaction.questions[0].id.clone(),
                labels: vec!["Blue".into()],
                other: None,
            }],
            note: Some("Keep it simple".into()),
        };
        let result = parser.response(&interaction.id, &response).unwrap();
        assert_eq!(result["id"], 0);
        assert_eq!(
            result["result"]["answers"][&interaction.questions[0].id]["answers"],
            json!(["Blue", "Note: Keep it simple"])
        );
        request["params"]["questions"][0]["options"] = Value::Null;
        request["params"]["questions"][0]["isSecret"] = json!(true);
        let events = parser.parse(&request);
        let AgentEvent::NeedsInput { interaction } = &events[0] else {
            panic!("No card")
        };
        assert!(interaction.questions[0].allow_other);
        assert!(interaction.questions[0].secret);
    }
    #[test]
    fn real_plain_interrupt_resume_and_error_fixtures() {
        for trace in [
            include_str!("../../tests/fixtures/codex/plain.jsonl"),
            include_str!("../../tests/fixtures/codex/resume.jsonl"),
        ] {
            let events = replay(trace);
            assert!(matches!(
                events.last(),
                Some(AgentEvent::TurnFinished {
                    outcome: TurnOutcome::Completed,
                    reply: Some(_),
                    ..
                })
            ));
        }
        let events = replay(include_str!("../../tests/fixtures/codex/interrupt.jsonl"));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnFinished {
                outcome: TurnOutcome::Interrupted,
                ..
            })
        ));
        let events = replay(include_str!("../../tests/fixtures/codex/error.jsonl"));
        assert!(events.iter().any(
            |event| matches!(event, AgentEvent::Error { message } if message.contains("model"))
        ));
    }
    #[test]
    fn real_file_edit_fixture_finishes() {
        let events = replay(include_str!("../../tests/fixtures/codex/edit.jsonl"));
        assert!(events.iter().any(
            |event| matches!(event, AgentEvent::StepStarted { step } if step.kind == StepKind::Edit)
        ));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnFinished {
                outcome: TurnOutcome::Completed,
                reply: Some(_),
                ..
            })
        ));
    }
    #[test]
    fn paths_and_unknown_items_have_compact_titles() {
        assert_eq!(
            item_title(
                &json!({"type":"fileChange","changes":[{"path":"/scratch/test.txt"}]}),
                "/scratch"
            )
            .1,
            "Editing test.txt"
        );
        assert_eq!(
            item_title(&json!({"type":"futureTool"}), "/scratch").0,
            StepKind::Other
        );
    }
}
