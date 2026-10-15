use super::{
    process::{Output, Process},
    *,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Duration,
};
use tokio::{sync::mpsc, time::Instant};

#[derive(Default)]
pub struct Parser {
    pub requests: HashMap<String, Value>,
    unknown: HashSet<String>,
    last_text: String,
    model: Option<String>,
    used: u64,
    window: Option<u64>,
}
fn string(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}
fn text_content(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.into();
    }
    value
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b.as_str().or_else(|| b["text"].as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
fn context_tokens(usage: &Value) -> Option<u64> {
    usage.is_object().then(|| {
        [
            "input_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
            "output_tokens",
        ]
        .iter()
        .map(|key| usage[*key].as_u64().unwrap_or(0))
        .sum()
    })
}
impl Parser {
    fn context_window(&self, model_usage: &Value) -> Option<u64> {
        let windows = model_usage.as_object()?.iter().filter_map(|(name, usage)| {
            usage["contextWindow"]
                .as_u64()
                .filter(|window| *window > 0)
                .map(|window| (name, window))
        });
        windows
            .clone()
            .find(|(name, _)| Some(name.as_str()) == self.model.as_deref())
            .map(|(_, window)| window)
            .or_else(|| windows.map(|(_, window)| window).max())
    }
    pub fn parse(&mut self, msg: &Value, cwd: &Path) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        match msg["type"].as_str().unwrap_or_default() {
            "system" if msg["subtype"] == "init" => {
                self.model = msg["model"].as_str().map(str::to_string);
                events.push(AgentEvent::SessionReady {
                    native_id: string(msg, "session_id"),
                    model: self.model.clone(),
                })
            }
            "assistant" => {
                let parent_id = msg["parent_tool_use_id"].as_str().map(str::to_string);
                if parent_id.is_none()
                    && let Some(used) = context_tokens(&msg["message"]["usage"])
                {
                    self.used = used;
                    events.push(AgentEvent::Usage {
                        used_tokens: used,
                        window_tokens: self.window,
                    });
                }
                if let Some(blocks) = msg["message"]["content"].as_array() {
                    for (index, block) in blocks.iter().enumerate() {
                        match block["type"].as_str().unwrap_or_default() {
                            "text" => {
                                let text = string(block, "text");
                                self.last_text = text.clone();
                                events.push(AgentEvent::Commentary {
                                    id: format!("{}:{index}", string(&msg["message"], "id")),
                                    text,
                                });
                            }
                            "thinking" => {
                                let id =
                                    format!("{}:thinking:{index}", string(&msg["message"], "id"));
                                events.push(AgentEvent::StepStarted {
                                    step: Step {
                                        id: id.clone(),
                                        parent_id: parent_id.clone(),
                                        kind: StepKind::Thinking,
                                        title: "Thinking".into(),
                                        input: json!({"text":block["thinking"]}),
                                    },
                                });
                                events.push(AgentEvent::StepFinished {
                                    id,
                                    status: StepStatus::Done,
                                    output: None,
                                });
                            }
                            "tool_use" => {
                                let input = block["input"].clone();
                                let (kind, title) =
                                    oneliner::title(&string(block, "name"), &input, cwd);
                                events.push(AgentEvent::StepStarted {
                                    step: Step {
                                        id: string(block, "id"),
                                        parent_id: parent_id.clone(),
                                        kind,
                                        title,
                                        input,
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                }
            }
            "user" => {
                if let Some(blocks) = msg["message"]["content"].as_array() {
                    for block in blocks {
                        if block["type"] == "tool_result" {
                            events.push(AgentEvent::StepFinished {
                                id: string(block, "tool_use_id"),
                                status: if block["is_error"] == true {
                                    StepStatus::Failed
                                } else {
                                    StepStatus::Done
                                },
                                output: Some(cap_output(&text_content(&block["content"]))),
                            })
                        }
                    }
                }
            }
            "control_request" if msg["request"]["subtype"] == "can_use_tool" => {
                let id = string(msg, "request_id");
                let req = msg["request"].clone();
                let input = &req["input"];
                let tool = string(&req, "tool_name");
                self.requests.insert(id.clone(), req.clone());
                let mut interaction = Interaction {
                    id,
                    kind: "approval".into(),
                    title: format!("Allow Claude to use {tool}?"),
                    questions: Vec::new(),
                    approval: None,
                    plan: None,
                    can_dismiss: true,
                    max_text_length: None,
                };
                if tool == "AskUserQuestion" {
                    interaction.kind = "question".into();
                    interaction.title = "Claude has a question".into();
                    let questions = if let Some(text) = input["questions"].as_str() {
                        serde_json::from_str(text).unwrap_or(Value::Null)
                    } else {
                        input["questions"].clone()
                    };
                    if let Some(questions) = questions.as_array() {
                        interaction.questions = questions
                            .iter()
                            .enumerate()
                            .map(|(i, q)| Question {
                                id: i.to_string(),
                                header: string(q, "header"),
                                text: string(q, "question"),
                                options: q["options"]
                                    .as_array()
                                    .map(|opts| {
                                        opts.iter()
                                            .map(|o| QOption {
                                                label: string(o, "label"),
                                                description: string(o, "description"),
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                multi: q["multiSelect"] == true,
                                allow_other: true,
                                secret: false,
                            })
                            .collect();
                    }
                } else if tool == "ExitPlanMode" {
                    interaction.kind = "plan".into();
                    interaction.title = "Start working on this plan?".into();
                    interaction.plan =
                        Some(input["plan"].as_str().unwrap_or(&self.last_text).into());
                } else {
                    interaction.approval = Some(Approval {
                        summary: input["description"].as_str().unwrap_or(&tool).into(),
                        detail: serde_json::to_string_pretty(input).unwrap_or_default(),
                        choices: vec![
                            Choice {
                                id: "allow".into(),
                                label: "Allow".into(),
                                kind: "allow".into(),
                            },
                            Choice {
                                id: "deny".into(),
                                label: "Decline".into(),
                                kind: "deny".into(),
                            },
                        ],
                    });
                }
                events.push(AgentEvent::NeedsInput { interaction });
            }
            "control_cancel_request" => events.push(AgentEvent::InteractionClosed {
                id: string(msg, "request_id"),
            }),
            "result" => {
                self.window = self.context_window(&msg["modelUsage"]).or(self.window);
                if self.used > 0 {
                    events.push(AgentEvent::Usage {
                        used_tokens: self.used,
                        window_tokens: self.window,
                    });
                }
                events.push(AgentEvent::TurnFinished {
                    outcome: if msg["is_error"] == true
                        || msg["subtype"]
                            .as_str()
                            .is_some_and(|s| s.starts_with("error"))
                    {
                        TurnOutcome::Failed {
                            message: msg["result"]
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| text_content(&msg["errors"])),
                        }
                    } else {
                        TurnOutcome::Completed
                    },
                    reply: msg["result"].as_str().map(str::to_owned),
                    duration_ms: msg["duration_ms"].as_u64().unwrap_or(0),
                })
            }
            "control_response" if msg["response"]["subtype"] == "error" => {
                events.push(AgentEvent::Error {
                    message: string(&msg["response"], "error"),
                })
            }
            "system" | "control_response" | "rate_limit_event" | "stream_event" => {}
            name => {
                if self.unknown.insert(name.into()) {
                    tracing::warn!(event_type = name, "Unknown Claude event");
                }
            }
        }
        events
    }
    pub fn response(
        &mut self,
        id: &str,
        response: &InteractionResponse,
    ) -> Result<(Value, bool), String> {
        let req = self
            .requests
            .remove(id)
            .ok_or("This request expired. Retry the conversation.")?;
        let mut input = req["input"].clone();
        let allow = match response {
            InteractionResponse::Answers { .. } => true,
            InteractionResponse::Choice { choice_id, .. } => choice_id == "allow",
            InteractionResponse::Dismiss => false,
        };
        if let InteractionResponse::Answers { answers, note } = response {
            let questions = if let Some(text) = input["questions"].as_str() {
                serde_json::from_str::<Value>(text).unwrap_or(Value::Null)
            } else {
                input["questions"].clone()
            };
            let mut values = serde_json::Map::new();
            for answer in answers {
                let index: usize = answer
                    .question_id
                    .parse()
                    .map_err(|_| "Invalid question id")?;
                let question = questions[index]["question"]
                    .as_str()
                    .ok_or("Unknown question")?;
                let mut text = answer
                    .other
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| answer.labels.join(", "));
                if let Some(note) = note.as_deref().filter(|s| !s.is_empty()) {
                    text.push_str(&format!(" (note: {note})"));
                }
                values.insert(question.into(), Value::String(text));
            }
            input["answers"] = Value::Object(values);
        }
        let reply = if allow {
            json!({"behavior":"allow","updatedInput":input})
        } else {
            json!({"behavior":"deny","message":match response{InteractionResponse::Choice{feedback,..}=>feedback.as_deref().unwrap_or("The user declined this request."),_=>"The user dismissed the question."}})
        };
        Ok((
            json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":reply}}),
            allow && req["tool_name"] == "ExitPlanMode",
        ))
    }
}
fn args(settings: &TurnSettings, native_id: &str, resume: bool) -> Vec<String> {
    let mode = match settings.permission {
        Permission::Yolo => "bypassPermissions",
        Permission::Ask => "manual",
        Permission::Plan => "plan",
    };
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--allow-dangerously-skip-permissions",
        "--permission-prompt-tool",
        "stdio",
        "--permission-mode",
        mode,
        if resume { "--resume" } else { "--session-id" },
        native_id,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    if let Some(model) = &settings.model {
        args.extend(["--model".into(), model.clone()]);
    }
    if let Some(effort) = &settings.effort {
        args.extend(["--effort".into(), effort.clone()]);
    }
    args
}
// Claude Code 2.1.300 offers TodoWrite only with these two switches. Its default
// task tools (TaskCreate, TaskUpdate) are not read by the To do panel.
fn enable_todo_write(env: &mut HashMap<String, String>) {
    env.insert("CLAUDE_CODE_ENABLE_TASKS".into(), "false".into());
    env.insert("CLAUDE_CODE_ENABLE_TODO_TOOLS".into(), "true".into());
}
pub async fn run(
    mut context: StartContext,
    mut commands: mpsc::Receiver<AgentCommand>,
    events: mpsc::Sender<AgentEvent>,
) {
    enable_todo_write(&mut context.env);
    let mut process: Option<Process> = None;
    let mut parser = Parser::default();
    let mut active = false;
    let mut stop_deadline: Option<Instant> = None;
    loop {
        let deadline = stop_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(86400));
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(AgentCommand::Send { turn_id, text, settings }) => {
                        if active {
                            let _ = events.send(AgentEvent::Error { message: "A turn is already running".into() }).await;
                            continue;
                        }
                        if context.settings != settings {
                            if let Some(mut old) = process.take() { old.terminate().await; }
                            context.settings = settings;
                        }
                        if process.is_none() {
                            let resume = context.native_id.is_some();
                            let native_id = context.native_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                            match Process::spawn(&context, &args(&context.settings, &native_id, resume)) {
                                Ok(mut child) => {
                                    context.native_id = Some(native_id.clone());
                                    let _ = events.send(AgentEvent::SessionReady { native_id, model: context.settings.model.clone() }).await;
                                    if let Err(message) = child.write(&json!({
                                        "type": "control_request", "request_id": "init-1", "request": {"subtype": "initialize"}
                                    })).await {
                                        let _ = events.send(AgentEvent::Error { message }).await;
                                    }
                                    process = Some(child);
                                    parser = Parser::default();
                                }
                                Err(message) => {
                                    let _ = events.send(AgentEvent::TurnFinished {
                                        outcome: TurnOutcome::Failed { message }, reply: None, duration_ms: 0,
                                    }).await;
                                    continue;
                                }
                            }
                        }
                        active = true;
                        let _ = events.send(AgentEvent::TurnStarted { turn_id }).await;
                        if let Err(message) = process.as_mut().unwrap().write(&json!({
                            "type": "user", "message": {"role": "user", "content": text}
                        })).await {
                            active = false;
                            let _ = events.send(AgentEvent::TurnFinished {
                                outcome: TurnOutcome::Failed { message }, reply: None, duration_ms: 0,
                            }).await;
                            process.take();
                        }
                    }
                    Some(AgentCommand::Respond { interaction_id, response }) => {
                        match parser.response(&interaction_id, &response) {
                            Ok((reply, start_working)) => {
                                if let Some(child) = &mut process {
                                    if start_working {
                                        context.settings.permission = Permission::Ask;
                                        let _ = child.write(&json!({
                                            "type": "control_request", "request_id": uuid::Uuid::new_v4().to_string(),
                                            "request": {"subtype": "set_permission_mode", "mode": "manual"}
                                        })).await;
                                    }
                                    if let Err(message) = child.write(&reply).await {
                                        let _ = events.send(AgentEvent::Error { message }).await;
                                    }
                                }
                            }
                            Err(message) => { let _ = events.send(AgentEvent::Error { message }).await; }
                        }
                    }
                    Some(AgentCommand::Interrupt) => {
                        if active {
                            if let Some(child) = &mut process {
                                let _ = child.write(&json!({
                                    "type": "control_request", "request_id": uuid::Uuid::new_v4().to_string(),
                                    "request": {"subtype": "interrupt"}
                                })).await;
                            }
                            stop_deadline = Some(Instant::now() + Duration::from_secs(5));
                        }
                    }
                    Some(AgentCommand::Shutdown) | None => break,
                }
            }
            output = async {
                match &mut process {
                    Some(p) => p.output.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                match output {
                    Some(Output::Message(msg)) => {
                        for mut event in parser.parse(&msg, &context.cwd) {
                            // Background tasks can send another result after the user's turn ended.
                            if !active && !matches!(event, AgentEvent::SessionReady { .. } | AgentEvent::Error { .. }) {
                                continue;
                            }
                            if let AgentEvent::SessionReady { native_id, .. } = &event {
                                context.native_id = Some(native_id.clone());
                            }
                            if matches!(event, AgentEvent::TurnFinished { .. }) {
                                active = false;
                                if stop_deadline.take().is_some() {
                                    event = AgentEvent::TurnFinished { outcome: TurnOutcome::Interrupted, reply: None, duration_ms: 0 };
                                }
                            }
                            if events.send(event).await.is_err() { return; }
                        }
                    }
                    Some(Output::Closed) | None => {
                        let stderr_tail = process.as_ref().map(Process::stderr_tail).unwrap_or_default();
                        process.take();
                        if active {
                            active = false;
                            let _ = events.send(AgentEvent::Exited { code: None, stderr_tail }).await;
                        }
                        stop_deadline = None;
                    }
                }
            }
            _ = tokio::time::sleep_until(deadline), if stop_deadline.is_some() => {
                if let Some(mut child) = process.take() { child.terminate().await; }
                active = false;
                stop_deadline = None;
                let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Interrupted, reply: None, duration_ms: 0 }).await;
            }
        }
    }
    if let Some(mut child) = process {
        child.terminate().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn todo_write_switches_are_set_for_the_child_process() {
        let mut env = HashMap::from([("PATH".to_string(), "/bin".to_string())]);
        enable_todo_write(&mut env);
        assert_eq!(env["CLAUDE_CODE_ENABLE_TASKS"], "false");
        assert_eq!(env["CLAUDE_CODE_ENABLE_TODO_TOOLS"], "true");
        assert_eq!(env["PATH"], "/bin");
    }
    fn replay(text: &str) -> Vec<AgentEvent> {
        let mut parser = Parser::default();
        let mut events = Vec::new();
        for line in text.lines() {
            let record: Value = serde_json::from_str(line).unwrap();
            if record["dir"] == "out" {
                events.extend(parser.parse(&record["msg"], Path::new("/scratch")));
            }
        }
        events
    }
    #[test]
    fn recorded_question_and_interaction_turns() {
        let question = replay(include_str!("../../tests/fixtures/claude/question.jsonl"));
        assert!(question.iter().any(|e|matches!(e,AgentEvent::NeedsInput{interaction} if interaction.kind=="question" && interaction.questions[0].options.len()==2)));
        assert!(question.iter().any(|e| matches!(
            e,
            AgentEvent::TurnFinished {
                outcome: TurnOutcome::Completed,
                ..
            }
        )));
        let turns = replay(include_str!(
            "../../tests/fixtures/claude/interaction-turns.jsonl"
        ));
        assert!(turns.iter().any(
            |event| matches!(event, AgentEvent::NeedsInput { interaction }
                if interaction.questions.iter().any(|question| question.multi)
            )
        ));
        let sessions: Vec<_> = turns
            .iter()
            .filter_map(|event| match event {
                AgentEvent::SessionReady { native_id, .. } => Some(native_id),
                _ => None,
            })
            .collect();
        assert!(sessions.len() > 1);
        assert!(sessions.iter().all(|session| *session == sessions[0]));
        let records: Vec<Value> =
            include_str!("../../tests/fixtures/claude/interaction-turns.jsonl")
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
        assert!(
            records.iter().any(|record| record["dir"] == "in"
                && record["msg"]["request"]["subtype"] == "interrupt")
        );
        for kind in ["approval", "plan"] {
            assert!(turns.iter().any(
                |e| matches!(e,AgentEvent::NeedsInput{interaction} if interaction.kind==kind)
            ));
        }
        assert!(
            turns
                .iter()
                .any(|e| matches!(e,AgentEvent::StepStarted{step} if step.kind==StepKind::Write))
        );
        assert!(turns.iter().any(|e| matches!(
            e,
            AgentEvent::StepFinished {
                status: StepStatus::Done,
                ..
            }
        )));
    }
    #[test]
    fn thinking_blocks_finish_as_soon_as_they_arrive() {
        let mut parser = Parser::default();
        let events = parser.parse(
            &json!({"type":"assistant","message":{"id":"m1","content":[{"type":"thinking","thinking":""}]}}),
            Path::new("/scratch"),
        );
        assert!(
            matches!(&events[0], AgentEvent::StepStarted { step } if step.kind == StepKind::Thinking && step.id == "m1:thinking:0")
        );
        assert!(
            matches!(&events[1], AgentEvent::StepFinished { id, status: StepStatus::Done, .. } if id == "m1:thinking:0")
        );
    }
    #[test]
    fn context_usage_follows_top_level_messages_and_learns_the_window() {
        let mut parser = Parser::default();
        parser.parse(
            &json!({"type":"system","subtype":"init","session_id":"s","model":"claude-haiku-5-5"}),
            Path::new("/scratch"),
        );
        let usage = json!({"input_tokens":2,"cache_creation_input_tokens":20,"cache_read_input_tokens":300,"output_tokens":4});
        let events = parser.parse(
            &json!({"type":"assistant","message":{"id":"m","usage":usage,"content":[]}}),
            Path::new("/scratch"),
        );
        assert!(matches!(
            events[..],
            [AgentEvent::Usage {
                used_tokens: 326,
                window_tokens: None
            }]
        ));
        let sub = parser.parse(
            &json!({"type":"assistant","parent_tool_use_id":"t","message":{"id":"n","usage":{"input_tokens":9000},"content":[]}}),
            Path::new("/scratch"),
        );
        assert!(sub.is_empty());
        let done = parser.parse(
            &json!({"type":"result","subtype":"success","is_error":false,"duration_ms":1,"result":"ok","modelUsage":{"claude-haiku-5-5":{"contextWindow":1000000},"other":{"contextWindow":200000}}}),
            Path::new("/scratch"),
        );
        assert!(matches!(
            done[..],
            [
                AgentEvent::Usage {
                    used_tokens: 326,
                    window_tokens: Some(1_000_000)
                },
                AgentEvent::TurnFinished { .. }
            ]
        ));
    }
    #[test]
    fn recorded_model_error_is_readable() {
        let events = replay(include_str!("../../tests/fixtures/claude/error.jsonl"));
        assert!(events.iter().any(|event| matches!(event,
            AgentEvent::TurnFinished { outcome: TurnOutcome::Failed { message }, .. }
            if message.contains("selected model")
        )));
    }
    #[test]
    fn string_questions_multiselect_note_dismiss_and_unknown_events() {
        let mut parser = Parser::default();
        let request = json!({"type":"control_request","request_id":"q","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","input":{"questions":"[{\"question\":\"Colors?\",\"header\":\"Color\",\"options\":[],\"multiSelect\":true}]"}}});
        let events = parser.parse(&request, Path::new("/scratch"));
        assert!(
            matches!(&events[0],AgentEvent::NeedsInput{interaction} if interaction.questions[0].multi)
        );
        let (response, _) = parser
            .response(
                "q",
                &InteractionResponse::Answers {
                    answers: vec![Answer {
                        question_id: "0".into(),
                        labels: vec!["Blue".into(), "Green".into()],
                        other: None,
                    }],
                    note: Some("A note".into()),
                },
            )
            .unwrap();
        assert_eq!(
            response["response"]["response"]["updatedInput"]["answers"]["Colors?"],
            "Blue, Green (note: A note)"
        );
        parser.parse(&request, Path::new("/scratch"));
        let (response, _) = parser.response("q", &InteractionResponse::Dismiss).unwrap();
        assert_eq!(response["response"]["response"]["behavior"], "deny");
        assert!(
            parser
                .parse(
                    &json!({"type":"futureEvent","unknownField":true}),
                    Path::new("/scratch")
                )
                .is_empty()
        );
        let events=parser.parse(&json!({"type":"result","subtype":"error_during_execution","is_error":true,"errors":["Bad model"]}),Path::new("/scratch"));
        assert!(
            matches!(&events[0],AgentEvent::TurnFinished{outcome:TurnOutcome::Failed{message},..} if message=="Bad model")
        );
        assert!(
            args(&TurnSettings::default(), "id", false)
                .contains(&"--allow-dangerously-skip-permissions".into())
        );
        let output = cap_output(&"é".repeat(20_000));
        assert!(output.len() <= 16_384);
        assert!(output.is_char_boundary(output.len()));
    }
}
