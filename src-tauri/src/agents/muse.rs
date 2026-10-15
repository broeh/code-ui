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

fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn mutation(mut params: Value) -> Value {
    params["commandId"] = json!(uuid::Uuid::now_v7().to_string());
    params
}
#[derive(Default)]
pub struct Parser {
    cwd: std::path::PathBuf,
    turn: String,
    kinds: HashMap<String, String>,
    started: HashSet<String>,
    requests: HashMap<String, Value>,
    reply: Option<(String, String)>,
}
impl Parser {
    pub fn new(cwd: std::path::PathBuf) -> Self {
        Self {
            cwd,
            ..Self::default()
        }
    }
    pub fn begin(&mut self, turn: &str) {
        self.turn = turn.into();
        self.kinds.clear();
        self.started.clear();
        self.requests.clear();
        self.reply = None;
    }
    pub fn parse(&mut self, msg: &Value) -> Vec<AgentEvent> {
        let method = msg["method"].as_str().unwrap_or_default();
        let params = &msg["params"];
        if method == "approval/request" || method == "userInput/request" {
            return self.request(method, params);
        }
        match method {
            "item/started" | "item/updated" | "item/completed" => {
                let item = &params["item"];
                let id = string(item, "itemId");
                let kind = string(item, "kind");
                self.kinds.insert(id.clone(), kind.clone());
                if kind == "userMessage" || kind == "reminderChild" {
                    return Vec::new();
                }
                let mut events = Vec::new();
                if kind == "agentMessage" {
                    if self
                        .reply
                        .as_ref()
                        .is_some_and(|(previous, _)| previous != &id)
                    {
                        self.commentary(&mut events);
                    }
                    self.reply = Some((id, string(item, "text")));
                    return events;
                }
                if kind == "toolCall" || kind == "userShell" {
                    self.commentary(&mut events);
                }
                if self.started.insert(id.clone()) {
                    let (step_kind, title, input) = match kind.as_str() {
                        "toolCall" => {
                            let raw = item["args"].as_str().unwrap_or("{}");
                            let input = serde_json::from_str(raw).unwrap_or(json!({"text":raw}));
                            let (kind, title) = oneliner::title(
                                item["tool"].as_str().unwrap_or("tool"),
                                &input,
                                &self.cwd,
                            );
                            (kind, title, input)
                        }
                        "reasoning" => (StepKind::Thinking, "Thinking".into(), item.clone()),
                        "userShell" => (
                            StepKind::Command,
                            format!("Running: {}", string(item, "commandText")),
                            item.clone(),
                        ),
                        "subagent" => (StepKind::Subagent, "Delegating work".into(), item.clone()),
                        "workflow" => (StepKind::Subagent, "Running workflow".into(), item.clone()),
                        "compaction" => (
                            StepKind::Compact,
                            "Compacting conversation".into(),
                            item.clone(),
                        ),
                        "webSearch" => (StepKind::Web, "Searching the web".into(), item.clone()),
                        _ => (
                            StepKind::Other,
                            format!(
                                "{kind}: {}",
                                item["fallbackText"].as_str().unwrap_or("Activity")
                            ),
                            item.clone(),
                        ),
                    };
                    events.push(AgentEvent::StepStarted {
                        step: Step {
                            id: id.clone(),
                            parent_id: item["parentItemId"].as_str().map(str::to_owned),
                            kind: step_kind,
                            title: title.chars().take(80).collect(),
                            input,
                        },
                    });
                }
                if method == "item/completed" {
                    let status = match item["status"].as_str() {
                        Some("completed") => StepStatus::Done,
                        Some("rejected") => StepStatus::Declined,
                        Some("cancelled") => StepStatus::Interrupted,
                        _ => StepStatus::Failed,
                    };
                    let output = item["visibleOutput"].as_str().map(cap_output).or_else(|| {
                        item["summary"].as_array().map(|parts| {
                            cap_output(
                                &parts
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            )
                        })
                    });
                    events.push(AgentEvent::StepFinished { id, status, output });
                }
                events
            }
            "item/delta" => {
                let id = string(params, "itemId");
                let text = string(params, "delta");
                if self
                    .kinds
                    .get(&id)
                    .is_some_and(|kind| kind == "agentMessage")
                {
                    if let Some((_, reply)) = &mut self.reply {
                        reply.push_str(&text);
                    }
                    Vec::new()
                } else {
                    vec![AgentEvent::StepUpdated {
                        id,
                        title: None,
                        output_append: Some(text),
                    }]
                }
            }
            "session/contextUsage" => params["usedTokens"]
                .as_u64()
                .map(|used| {
                    vec![AgentEvent::Usage {
                        used_tokens: used,
                        window_tokens: params["windowTokens"].as_u64(),
                    }]
                })
                .unwrap_or_default(),
            "turn/completed" => vec![AgentEvent::TurnFinished {
                outcome: match params["terminal"].as_str() {
                    Some("completed") => TurnOutcome::Completed,
                    Some("cancelled") => TurnOutcome::Interrupted,
                    _ => TurnOutcome::Failed {
                        message: params["error"]["message"]
                            .as_str()
                            .or_else(|| params["reason"].as_str())
                            .unwrap_or("Muse could not finish this turn.")
                            .into(),
                    },
                },
                reply: self
                    .reply
                    .as_ref()
                    .map(|(_, text)| text.clone())
                    .filter(|text| !text.is_empty()),
                duration_ms: params["durationMs"].as_u64().unwrap_or(0),
            }],
            "approval/resolved" | "userInput/settled" => {
                let key = if method == "approval/resolved" {
                    "approvalId"
                } else {
                    "userInputId"
                };
                self.requests
                    .iter()
                    .filter(|(_, request)| request[key] == params[key])
                    .map(|(id, _)| AgentEvent::InteractionClosed { id: id.clone() })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
    fn commentary(&mut self, events: &mut Vec<AgentEvent>) {
        if let Some((id, text)) = self.reply.take()
            && !text.is_empty()
        {
            events.push(AgentEvent::Commentary { id, text });
        }
    }
    fn request(&mut self, method: &str, params: &Value) -> Vec<AgentEvent> {
        let approval = method == "approval/request";
        let native = string(
            params,
            if approval {
                "approvalId"
            } else {
                "userInputId"
            },
        );
        let id = if approval {
            format!(
                "{}:muse:{native}:{}",
                self.turn, params["currentRequirementId"]["sourceIndex"]
            )
        } else {
            format!("{}:muse:{native}", self.turn)
        };
        self.requests.insert(id.clone(), params.clone());
        let interaction = Interaction {
            id,
            kind: if approval { "approval" } else { "question" }.into(),
            title: if approval {
                format!(
                    "Allow Muse to use {}?",
                    params["toolName"].as_str().unwrap_or("this tool")
                )
            } else {
                "Muse has a question".into()
            },
            questions: if approval {
                Vec::new()
            } else {
                params["questions"]
                    .as_array()
                    .map(|questions| {
                        questions
                            .iter()
                            .map(|q| Question {
                                id: string(q, "id"),
                                header: string(q, "header"),
                                text: string(q, "question"),
                                options: q["options"]
                                    .as_array()
                                    .map(|options| {
                                        options
                                            .iter()
                                            .map(|o| QOption {
                                                label: string(o, "label"),
                                                description: string(o, "description"),
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                multi: q["selection"]["mode"] == "multiple",
                                allow_other: true,
                                secret: false,
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            },
            approval: approval.then(|| Approval {
                summary: format!("Muse wants to use {}", string(params, "toolName")),
                detail: string(params, "rawArgs"),
                choices: params["availableChoices"]
                    .as_array()
                    .map(|choices| {
                        choices
                            .iter()
                            .map(|c| Choice {
                                id: string(c, "choiceId"),
                                label: string(c, "label"),
                                kind: if c["decision"]
                                    .as_str()
                                    .is_some_and(|d| d.starts_with("approved"))
                                {
                                    if c["scope"] == "session" {
                                        "allowSession"
                                    } else {
                                        "allow"
                                    }
                                } else {
                                    "deny"
                                }
                                .into(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }),
            plan: None,
            can_dismiss: !approval,
            max_text_length: (!approval).then_some(500),
        };
        vec![AgentEvent::NeedsInput { interaction }]
    }
    pub fn response(
        &self,
        id: &str,
        response: &InteractionResponse,
    ) -> Result<(&'static str, Value), String> {
        let request = self
            .requests
            .get(id)
            .ok_or("This request expired. Retry the conversation.")?;
        let session = &request["sessionId"];
        if let Some(native) = request.get("approvalId") {
            let InteractionResponse::Choice {
                choice_id,
                feedback,
            } = response
            else {
                return Err("Choose a Muse approval option.".into());
            };
            let choice = request["availableChoices"]
                .as_array()
                .and_then(|choices| {
                    choices
                        .iter()
                        .find(|choice| choice["choiceId"] == *choice_id)
                })
                .ok_or("This approval choice is no longer available.")?;
            let mut params = mutation(
                json!({"sessionId":session,"approvalId":native,"choiceId":choice_id,"requirementId":request["currentRequirementId"]}),
            );
            if choice["acceptsFeedback"] == true
                && let Some(feedback) = feedback
            {
                params["feedback"] = json!(feedback);
            }
            Ok(("approval/decide", params))
        } else {
            let mut params =
                mutation(json!({"sessionId":session,"userInputId":request["userInputId"]}));
            if matches!(response, InteractionResponse::Dismiss) {
                params["reason"] = json!("The user dismissed this question.");
                return Ok(("userInput/cancel", params));
            }
            let InteractionResponse::Answers { answers, note } = response else {
                return Err("Answer the Muse question to continue.".into());
            };
            let mut rows = Vec::new();
            for answer in answers {
                let question = request["questions"]
                    .as_array()
                    .and_then(|questions| questions.iter().find(|q| q["id"] == answer.question_id))
                    .ok_or("This question expired.")?;
                let mut row = json!({"questionId":answer.question_id});
                if let Some(other) = answer.other.as_deref().filter(|text| !text.is_empty()) {
                    row["freeText"] = json!(other);
                } else if question["selection"]["mode"] == "multiple" {
                    row["selectedLabels"] = json!(answer.labels);
                } else {
                    row["selectedLabel"] = json!(answer.labels.first().ok_or("Select an answer.")?);
                }
                if let Some(note) = note.as_deref().filter(|text| !text.is_empty()) {
                    row["note"] = json!(note);
                }
                rows.push(row);
            }
            params["answers"] = json!(rows);
            Ok(("userInput/answer", params))
        }
    }
}
async fn connect(context: &StartContext) -> Result<(Client, String, Option<String>), String> {
    let mut args = vec!["serve".into()];
    if context.settings.permission == Permission::Yolo {
        args.extend(["--disable-sandbox".into(), "--trust-workspace".into()]);
    }
    let mut client = Client::spawn(context, &args)?;
    client.router.jsonrpc = true;
    client.call("initialize", json!({"clientInfo":{"name":"code_ui","title":"CODE_UI","version":env!("CARGO_PKG_VERSION")},"capabilities":{"userInputDialogs":true}})).await?;
    client
        .write(&json!({"jsonrpc":"2.0","method":"initialized"}))
        .await?;
    let result = if let Some(native) = &context.native_id {
        client
            .call(
                "session/resume",
                mutation(json!({"sessionId":native,"excludeItems":true})),
            )
            .await?
    } else {
        let mut params = mutation(
            json!({"workspaceRoot":context.cwd,"approvalMode":if context.settings.permission == Permission::Yolo {"allowAll"}else{"onRequest"}}),
        );
        if let Some(model) = &context.settings.model {
            params["modelId"] = json!(model);
        }
        client.call("session/start", params).await?
    };
    let session = &result["session"];
    let native = string(session, "sessionId");
    if native.is_empty() {
        return Err("Muse returned no session id.".into());
    }
    Ok((
        client,
        native,
        context
            .settings
            .model
            .clone()
            .or_else(|| session["modelId"].as_str().map(str::to_owned)),
    ))
}
pub async fn run(
    mut context: StartContext,
    mut commands: mpsc::Receiver<AgentCommand>,
    events: mpsc::Sender<AgentEvent>,
) {
    let mut client: Option<Client> = None;
    let mut native = context.native_id.clone().unwrap_or_default();
    let mut native_turn = String::new();
    let mut parser = Parser::new(context.cwd.clone());
    let mut active = false;
    let mut deadline: Option<Instant> = None;
    loop {
        let stop_at = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(86400));
        tokio::select! {
            command = commands.recv() => match command {
                Some(AgentCommand::Send { turn_id, text, settings }) => {
                    if settings.permission == Permission::Plan {
                        let _ = events.send(AgentEvent::TurnFinished { outcome:TurnOutcome::Failed { message:"Muse does not support Plan permission mode.".into() },reply:None,duration_ms:0 }).await;
                        continue;
                    }
                    if active { continue; }
                    if context.settings.permission != settings.permission && let Some(mut old) = client.take() { old.shutdown().await; }
                    context.settings = settings.clone();
                    if client.is_none() {
                        match connect(&context).await {
                            Ok((rpc,id,model)) => {
                                client = Some(rpc);native=id;context.native_id=Some(native.clone());
                                let _ = events.send(AgentEvent::SessionReady { native_id:native.clone(), model }).await;
                            }
                            Err(message) => { let _ = events.send(AgentEvent::TurnFinished { outcome:TurnOutcome::Failed { message },reply:None,duration_ms:0 }).await;continue; }
                        }
                    }
                    let rpc = client.as_mut().unwrap();
                    let setup = async {
                        rpc.call("session/setApprovalMode",mutation(json!({"sessionId":native,"mode":if settings.permission == Permission::Yolo {"allowAll"}else{"onRequest"}}))).await?;
                        if let Some(model) = &settings.model { rpc.call("session/setModel",mutation(json!({"sessionId":native,"model":{"modelId":model}}))).await?; }
                        let mut params = mutation(json!({"sessionId":native,"input":[{"type":"text","text":text}],"ifBusy":"queue"}));
                        if let Some(effort) = &settings.effort { params["reasoningEffort"] = json!(effort); }
                        rpc.call("turn/start",params).await
                    }.await;
                    match setup {
                        Ok(result) => {
                            native_turn=string(&result,"turnId");parser.begin(&turn_id);active=true;
                            if settings.model.is_some() { let _ = events.send(AgentEvent::SessionReady { native_id:native.clone(), model:settings.model.clone() }).await; }
                            let _ = events.send(AgentEvent::TurnStarted { turn_id }).await;
                        }
                        Err(message) => { let _ = events.send(AgentEvent::TurnFinished { outcome:TurnOutcome::Failed { message },reply:None,duration_ms:0 }).await; }
                    }
                }
                Some(AgentCommand::Respond { interaction_id, response }) => {
                    match parser.response(&interaction_id,&response) {
                        Ok((method,params)) => {
                            if let Some(client) = &mut client && let Err(message) = client.call(method,params).await { let _ = events.send(AgentEvent::InteractionRejected { id:interaction_id, message }).await; }
                        }
                        Err(message) => { let _ = events.send(AgentEvent::InteractionRejected { id:interaction_id, message }).await; }
                    }
                }
                Some(AgentCommand::Interrupt) => {
                    if active {
                        if let Some(client) = &mut client { let _ = client.send("turn/interrupt",mutation(json!({"sessionId":native,"turnId":native_turn}))).await; }
                        deadline=Some(Instant::now()+Duration::from_secs(5));
                    }
                }
                Some(AgentCommand::Shutdown) | None => break,
            },
            msg = async { match &mut client { Some(client)=>client.next().await,None=>std::future::pending().await } } => {
                let Some(msg) = msg else {
                    let tail=client.as_ref().map(|c|c.process.stderr_tail()).unwrap_or_default();client=None;deadline=None;
                    if active { active=false;let _ = events.send(AgentEvent::Exited { code:None,stderr_tail:tail }).await; }
                    continue;
                };
                match client.as_mut().unwrap().router.route(msg.clone()) {
                    Message::Request(_) if ["approval/request","userInput/request"].contains(&msg["method"].as_str().unwrap_or_default()) => {
                        let _ = client.as_mut().unwrap().write(&json!({"jsonrpc":"2.0","id":msg["id"],"result":{}})).await;
                        if active { for event in parser.parse(&msg) { let _ = events.send(event).await; } }
                    }
                    Message::Request(_) => { let _ = client.as_mut().unwrap().write(&json!({"jsonrpc":"2.0","id":msg["id"],"error":{"code":-32601,"message":"Unsupported client request"}})).await; }
                    Message::Response { result:Err(message),.. } => { let _ = events.send(AgentEvent::Error { message }).await; }
                    Message::Notification(_) if active => {
                        let event_turn = msg["params"]["turnId"].as_str().or_else(|| msg["params"]["item"]["turnId"].as_str());
                        if event_turn.is_some_and(|turn| turn != native_turn) { continue; }
                        for mut event in parser.parse(&msg) {
                            if matches!(event,AgentEvent::TurnFinished { .. }) {
                                active=false;
                                if deadline.take().is_some() { event=AgentEvent::TurnFinished { outcome:TurnOutcome::Interrupted,reply:None,duration_ms:0 }; }
                            }
                            if events.send(event).await.is_err() { return; }
                        }
                    }
                    _ => {}
                }
            },
            _ = tokio::time::sleep_until(stop_at), if deadline.is_some() => {
                if let Some(mut rpc) = client.take() { rpc.shutdown().await; }
                active=false;deadline=None;
                let _ = events.send(AgentEvent::TurnFinished { outcome:TurnOutcome::Interrupted,reply:None,duration_ms:0 }).await;
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
            agent: AgentKind::Muse,
            ..TurnSettings::default()
        },
        log_dir: crate::paths::data_dir()?.join("logs"),
    };
    let mut client = Client::spawn(&context, &["serve".into()])?;
    client.router.jsonrpc = true;
    let result=async {
        client.call("initialize",json!({"clientInfo":{"name":"code_ui","version":env!("CARGO_PKG_VERSION")}})).await?;
        client.write(&json!({"jsonrpc":"2.0","method":"initialized"})).await?;
        let catalog=client.call("model/list",json!({})).await?;
        Ok(catalog["models"].as_array().map(|models|models.iter().map(|model|json!({"id":model["modelId"],"label":model["displayLabel"],"isDefault":model["isDefault"],"efforts":model["variants"].as_array().cloned().unwrap_or_default()})).collect()).unwrap_or_default())
    }.await;
    client.shutdown().await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn replay(trace: &str) -> Vec<AgentEvent> {
        let mut parser = Parser::new("/scratch".into());
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
    fn context_usage_reports_used_tokens_and_window() {
        let events = replay(include_str!("../../tests/fixtures/muse/plain.jsonl"));
        assert!(events.iter().any(|event| matches!(
            event,
            AgentEvent::Usage {
                used_tokens,
                window_tokens: Some(window)
            } if *used_tokens > 0 && window > used_tokens
        )));
    }
    fn request(trace: &str, method: &str) -> Value {
        trace
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|row| row["dir"] == "out" && row["msg"]["method"] == method)
            .unwrap()["msg"]
            .clone()
    }
    #[test]
    fn real_plain_read_and_write_fixtures_keep_final_reply() {
        for trace in [
            include_str!("../../tests/fixtures/muse/plain.jsonl"),
            include_str!("../../tests/fixtures/muse/read.jsonl"),
            include_str!("../../tests/fixtures/muse/edit.jsonl"),
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
        let events = replay(include_str!("../../tests/fixtures/muse/read.jsonl"));
        assert!(events.iter().any(|event|matches!(event,AgentEvent::StepStarted {step} if step.kind == StepKind::Read && step.title == "Reading README.md")));
        let events = replay(include_str!("../../tests/fixtures/muse/edit.jsonl"));
        assert!(events.iter().any(
            |event| matches!(event,AgentEvent::StepStarted {step} if step.kind == StepKind::Write)
        ));
        assert!(!events.iter().any(
            |event| matches!(event,AgentEvent::StepStarted {step} if step.kind == StepKind::Other)
        ));
    }
    #[test]
    fn real_resume_cancel_interrupt_and_model_error() {
        for trace in [
            include_str!("../../tests/fixtures/muse/resume.jsonl"),
            include_str!("../../tests/fixtures/muse/cancel.jsonl"),
        ] {
            let events = replay(trace);
            assert!(matches!(
                events.last(),
                Some(AgentEvent::TurnFinished {
                    outcome: TurnOutcome::Completed,
                    ..
                })
            ));
        }
        let events = replay(include_str!("../../tests/fixtures/muse/interrupt.jsonl"));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnFinished {
                outcome: TurnOutcome::Interrupted,
                ..
            })
        ));
        let events = replay(include_str!("../../tests/fixtures/muse/error.jsonl"));
        assert!(
            matches!(events.last(), Some(AgentEvent::TurnFinished { outcome:TurnOutcome::Failed { message }, .. }) if message.contains("code-ui-deliberately-invalid-model"))
        );
    }
    #[test]
    fn reminder_children_are_hidden() {
        let trace = include_str!("../../tests/fixtures/muse/error.jsonl");
        assert!(trace.contains("reminderChild"));
        assert!(!replay(trace).iter().any(|event| matches!(
            event,
            AgentEvent::StepStarted { .. } | AgentEvent::StepFinished { .. }
        )));
    }
    #[test]
    fn write_todos_becomes_a_task_list_step() {
        let mut parser = Parser::default();
        parser.begin("app");
        let args = json!({"todos":[{"status":"pending","text":"Build"}]});
        let events = parser.parse(&json!({"method":"item/started","params":{"item":{
            "itemId":"todo-1","kind":"toolCall","tool":"write_todos","args":args.to_string()
        }}}));
        let [AgentEvent::StepStarted { step }] = events.as_slice() else {
            panic!("Expected one step")
        };
        assert_eq!(step.kind, StepKind::Todo);
        assert_eq!(step.title, "Updating the task list");
        assert_eq!(step.input, args);
    }
    #[test]
    fn approval_choices_and_requirement_object_are_preserved() {
        let msg = request(
            include_str!("../../tests/fixtures/muse/approval.jsonl"),
            "approval/request",
        );
        let mut parser = Parser::default();
        parser.begin("app");
        let events = parser.parse(&msg);
        let AgentEvent::NeedsInput { interaction } = &events[0] else {
            panic!("No card")
        };
        assert!(!interaction.can_dismiss);
        assert_eq!(interaction.approval.as_ref().unwrap().choices.len(), 2);
        for choice in [
            &msg["params"]["availableChoices"][0]["choiceId"],
            &msg["params"]["availableChoices"][1]["choiceId"],
        ] {
            let (method, params) = parser
                .response(
                    &interaction.id,
                    &InteractionResponse::Choice {
                        choice_id: choice.as_str().unwrap().into(),
                        feedback: Some("No writes".into()),
                    },
                )
                .unwrap();
            assert_eq!(method, "approval/decide");
            assert_eq!(params["choiceId"], *choice);
            assert_eq!(
                params["requirementId"],
                msg["params"]["currentRequirementId"]
            );
            assert_eq!(
                uuid::Uuid::parse_str(params["commandId"].as_str().unwrap())
                    .unwrap()
                    .get_version_num(),
                7
            );
        }
        let mut next = msg.clone();
        next["params"]["currentRequirementId"]["sourceIndex"] = json!(1);
        let next = parser.parse(&next);
        let AgentEvent::NeedsInput { interaction: next } = &next[0] else {
            panic!("No card")
        };
        assert_ne!(interaction.id, next.id);
    }
    #[test]
    fn question_answers_multiselect_freeform_and_cancel_commands() {
        let mut msg = request(
            include_str!("../../tests/fixtures/muse/question.jsonl"),
            "userInput/request",
        );
        let mut parser = Parser::default();
        parser.begin("app");
        let events = parser.parse(&msg);
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
        let (method, params) = parser.response(&interaction.id, &response).unwrap();
        assert_eq!(method, "userInput/answer");
        assert_eq!(params["answers"][0]["selectedLabel"], "Blue");
        assert_eq!(params["answers"][0]["note"], "Keep it simple");
        let (method, params) = parser
            .response(&interaction.id, &InteractionResponse::Dismiss)
            .unwrap();
        assert_eq!(method, "userInput/cancel");
        assert_eq!(params["reason"], "The user dismissed this question.");
        msg["params"]["questions"][0]["selection"]["mode"] = json!("multiple");
        parser.parse(&msg);
        let (_, params) = parser.response(&interaction.id, &response).unwrap();
        assert_eq!(params["answers"][0]["selectedLabels"], json!(["Blue"]));
        let response = InteractionResponse::Answers {
            answers: vec![Answer {
                question_id: interaction.questions[0].id.clone(),
                labels: vec![],
                other: Some("Violet".into()),
            }],
            note: None,
        };
        let (_, params) = parser.response(&interaction.id, &response).unwrap();
        assert_eq!(params["answers"][0]["freeText"], "Violet");
    }
}
