use super::{
    process::{Output, Process},
    *,
};
use serde_json::{Value, json};
use std::{collections::HashSet, path::Path, time::Duration};
use tokio::sync::mpsc;

pub struct Parser {
    cwd: std::path::PathBuf,
    started: HashSet<String>,
    finished: HashSet<String>,
    response: Option<(String, String)>,
}
impl Parser {
    pub fn new(cwd: std::path::PathBuf) -> Self {
        Self {
            cwd,
            started: HashSet::new(),
            finished: HashSet::new(),
            response: None,
        }
    }
    pub fn begin(&mut self) {
        self.started.clear();
        self.finished.clear();
        self.response = None;
    }
    pub fn parse(&mut self, msg: &Value) -> Vec<AgentEvent> {
        match msg["event"].as_str().unwrap_or_default() {
            "init" => vec![AgentEvent::SessionReady {
                native_id: msg["conversation_id"].as_str().unwrap_or_default().into(),
                model: msg["init"]["model"].as_str().map(str::to_owned),
            }],
            "step_update" => {
                let step = &msg["step_update"];
                let id = step["step_index"].to_string();
                let kind = step["step_type"].as_str().unwrap_or("unknown");
                if kind == "user_input" {
                    return Vec::new();
                }
                let mut events = Vec::new();
                if kind == "agent_response" {
                    if let Some(used) = step["usage"]["total_tokens"].as_u64() {
                        events.push(AgentEvent::Usage {
                            used_tokens: used,
                            window_tokens: None,
                        });
                    }
                    if self
                        .response
                        .as_ref()
                        .is_some_and(|(previous, _)| previous != &id)
                    {
                        self.commentary(&mut events);
                    }
                    let (_, text) = self.response.get_or_insert((id, String::new()));
                    if let Some(delta) = step["text_delta"].as_str() {
                        text.push_str(delta);
                    }
                    return events;
                }
                self.commentary(&mut events);
                let info = &step["tool_info"];
                let tool = step["tool_name"]
                    .as_str()
                    .or_else(|| info["name"].as_str())
                    .unwrap_or(kind);
                let state = step["state"].as_str().unwrap_or_default();
                let parameters = if let Some(raw) = info["parameters"].as_str() {
                    serde_json::from_str(raw).unwrap_or(json!({"text":raw}))
                } else {
                    info["parameters"].clone()
                };
                // The stream carries only the path of the agent's task list, so read the file
                // once the write is done. A file without checkboxes shows as a normal write.
                if let Some(path) = task_list_path(tool, &parameters) {
                    if state == "ACTIVE" {
                        return events;
                    }
                    if let Some(todos) = read_task_list(&path)
                        && self.started.insert(id.clone())
                    {
                        self.finished.insert(id.clone());
                        events.push(AgentEvent::StepStarted {
                            step: Step {
                                id: id.clone(),
                                parent_id: None,
                                kind: StepKind::Todo,
                                title: "Updating the task list".into(),
                                input: json!({"todos":todos}),
                            },
                        });
                        events.push(AgentEvent::StepFinished {
                            id,
                            status: StepStatus::Done,
                            output: Some(format!("{} items", todos.len())),
                        });
                        return events;
                    }
                }
                if self.started.insert(id.clone()) {
                    let (kind, title) = oneliner::title(tool, &parameters, &self.cwd);
                    events.push(AgentEvent::StepStarted {
                        step: Step {
                            id: id.clone(),
                            parent_id: None,
                            kind,
                            title,
                            input: parameters,
                        },
                    });
                }
                if state != "ACTIVE" && self.finished.insert(id.clone()) {
                    let output = info.get("output").map(|output| {
                        cap_output(
                            &output
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| output.to_string()),
                        )
                    });
                    events.push(AgentEvent::StepFinished {
                        id,
                        status: if state == "DONE" {
                            StepStatus::Done
                        } else {
                            StepStatus::Failed
                        },
                        output,
                    });
                }
                events
            }
            "result" => {
                self.response = None;
                let result = &msg["result"];
                let reply = result["response"].as_str().map(str::to_owned);
                let outcome = if result["status"] == "SUCCESS" {
                    TurnOutcome::Completed
                } else if result["error"] == "interrupted" {
                    TurnOutcome::Interrupted
                } else {
                    TurnOutcome::Failed {
                        message: result["error"]
                            .as_str()
                            .or(reply.as_deref())
                            .unwrap_or("Antigravity could not finish this turn.")
                            .into(),
                    }
                };
                vec![AgentEvent::TurnFinished {
                    outcome,
                    reply,
                    duration_ms: (result["duration_seconds"].as_f64().unwrap_or(0.0) * 1000.0)
                        as u64,
                }]
            }
            other => {
                tracing::warn!(event = other, "Unknown Antigravity event");
                Vec::new()
            }
        }
    }
    fn commentary(&mut self, events: &mut Vec<AgentEvent>) {
        if let Some((id, text)) = self.response.take()
            && !text.trim().is_empty()
        {
            events.push(AgentEvent::Commentary { id, text });
        }
    }
}
fn task_list_path(tool: &str, parameters: &Value) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(parameters["TargetFile"].as_str()?);
    let name = path.file_name()?.to_str()?;
    (["write_to_file", "replace_file_content"].contains(&tool)
        && name.starts_with("task")
        && name.ends_with(".md")
        && path.components().any(|part| part.as_os_str() == "brain"))
    .then_some(path)
}
fn read_task_list(path: &Path) -> Option<Vec<Value>> {
    let todos: Vec<Value> = std::fs::read_to_string(path)
        .ok()?
        .lines()
        .filter_map(|line| {
            let item = line.trim_start().strip_prefix(['-', '*'])?.trim_start();
            let text = item.get(3..)?.trim();
            let status = match item.get(..3)? {
                "[ ]" => "pending",
                "[/]" => "in_progress",
                "[x]" | "[X]" => "completed",
                _ => return None,
            };
            (!text.is_empty()).then(|| json!({"text":text,"status":status}))
        })
        .collect();
    (!todos.is_empty()).then_some(todos)
}
fn arguments(settings: &TurnSettings, native: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "--input-format".into(),
        "stream-json".into(),
        "--output-format".into(),
        "stream-json".into(),
        "-p=".into(),
    ];
    match settings.permission {
        Permission::Yolo => args.push("--dangerously-skip-permissions".into()),
        Permission::Plan => args.extend(["--mode".into(), "plan".into()]),
        Permission::Ask => {}
    }
    if let Some(model) = &settings.model {
        args.extend(["--model".into(), model.clone()]);
    }
    if let Some(effort) = &settings.effort {
        args.extend(["--effort".into(), effort.clone()]);
    }
    if let Some(native) = native {
        args.extend(["--conversation".into(), native.into()]);
    }
    args
}
pub async fn run(
    mut context: StartContext,
    mut commands: mpsc::Receiver<AgentCommand>,
    events: mpsc::Sender<AgentEvent>,
) {
    let mut process: Option<Process> = None;
    let mut parser = Parser::new(context.cwd.clone());
    let mut active = false;
    let mut started_at = tokio::time::Instant::now();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(AgentCommand::Send { turn_id, text, settings }) => {
                    if active { continue; }
                    if context.settings != settings {
                        if let Some(mut previous) = process.take() { previous.terminate().await; }
                        context.settings = settings;
                    }
                    if process.is_none() {
                        match Process::spawn(&context, &arguments(&context.settings, context.native_id.as_deref())) {
                            Ok(child) => process = Some(child),
                            Err(message) => {
                                let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Failed { message }, reply: None, duration_ms: 0 }).await;
                                continue;
                            }
                        }
                    }
                    parser.begin(); active = true; started_at = tokio::time::Instant::now();
                    let _ = events.send(AgentEvent::TurnStarted { turn_id }).await;
                    if let Err(message) = process.as_mut().unwrap().write(&json!({"event":"user","message":{"role":"user","content":text}})).await {
                        active = false;
                        let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Failed { message }, reply: None, duration_ms: 0 }).await;
                    }
                }
                Some(AgentCommand::Interrupt) => {
                    if let Some(mut child) = process.take() { child.terminate().await; }
                    if active {
                        active = false;
                        let _ = events.send(AgentEvent::TurnFinished { outcome: TurnOutcome::Interrupted, reply: None, duration_ms: started_at.elapsed().as_millis() as u64 }).await;
                    }
                }
                Some(AgentCommand::Respond { .. }) => {
                    let _ = events.send(AgentEvent::Error { message: "Antigravity print mode has no question or approval response protocol. Reply in the message box.".into() }).await;
                }
                Some(AgentCommand::Shutdown) | None => break,
            },
            output = async { match &mut process { Some(child) => child.output.recv().await, None => std::future::pending().await } } => {
                match output {
                    Some(Output::Message(msg)) => {
                        if msg["event"] == "init" || active {
                            for mut event in parser.parse(&msg) {
                                if let AgentEvent::SessionReady { native_id, .. } = &event { context.native_id = Some(native_id.clone()); }
                                if let AgentEvent::TurnFinished { duration_ms, .. } = &mut event { active = false; *duration_ms = started_at.elapsed().as_millis() as u64; }
                                if events.send(event).await.is_err() { return; }
                            }
                        }
                    }
                    Some(Output::Closed) | None => {
                        let tail = process.as_ref().map(|child| child.stderr_tail()).unwrap_or_default();
                        process = None;
                        if active {
                            let _ = events.send(AgentEvent::Exited { code: None, stderr_tail: tail }).await;
                            break;
                        }
                    }
                }
            }
        }
    }
    if let Some(mut child) = process {
        child.terminate().await;
    }
}
pub async fn models(
    binary: &str,
    environment: &std::collections::HashMap<String, String>,
    cwd: &Path,
) -> Result<Vec<Value>, String> {
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(binary)
            .arg("models")
            .env_clear()
            .envs(environment)
            .current_dir(cwd)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "Antigravity model discovery timed out.")?
    .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(cap_output(&String::from_utf8_lossy(&output.stderr)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().filter_map(|line| line.split_once('\t')).map(|(id,label)|json!({"id":id,"label":label,"efforts":["low","medium","high","xhigh","max"]})).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn usage_events(events: &[AgentEvent]) -> Vec<u64> {
        events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Usage {
                    used_tokens,
                    window_tokens: None,
                } => Some(*used_tokens),
                _ => None,
            })
            .collect()
    }
    #[test]
    fn agent_response_usage_is_the_context_size() {
        let events = replay(include_str!("../../tests/fixtures/agy/read.jsonl"));
        assert_eq!(usage_events(&events), [16071, 16536]);
    }
    fn replay(trace: &str) -> Vec<AgentEvent> {
        let mut parser = Parser::new("/scratch".into());
        parser.begin();
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
    fn real_read_steps_and_final_result() {
        let events = replay(include_str!("../../tests/fixtures/agy/read.jsonl"));
        assert!(
            matches!(&events[0], AgentEvent::SessionReady { native_id, model: Some(_) } if !native_id.is_empty())
        );
        let starts: Vec<_> = events
            .iter()
            .filter_map(|event| {
                if let AgentEvent::StepStarted { step } = event {
                    Some(step)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0].kind, StepKind::Read);
        assert_eq!(starts[0].title, "Reading README.md");
        assert_eq!(starts[1].kind, StepKind::Command);
        assert!(
            matches!(events.last(), Some(AgentEvent::TurnFinished { outcome: TurnOutcome::Completed, reply: Some(text), duration_ms }) if text.contains("protocol test") && *duration_ms > 0)
        );
    }
    #[test]
    fn real_plan_fixture_finishes_without_host_approval_cards() {
        let events = replay(include_str!("../../tests/fixtures/agy/plan.jsonl"));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::NeedsInput { .. }))
        );
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
    fn print_question_falls_back_to_reply_without_an_unanswerable_card() {
        let events = replay(include_str!("../../tests/fixtures/agy/question.jsonl"));
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::NeedsInput { .. }))
        );
        assert!(events.iter().any(|event| matches!(event, AgentEvent::StepStarted { step } if step.kind == StepKind::Other)));
        assert!(
            matches!(events.last(), Some(AgentEvent::TurnFinished { reply: Some(text), .. }) if text.contains("red or blue"))
        );
    }
    #[test]
    fn real_edit_configured_and_resume_fixtures() {
        for trace in [
            include_str!("../../tests/fixtures/agy/edit.jsonl"),
            include_str!("../../tests/fixtures/agy/approval.jsonl"),
            include_str!("../../tests/fixtures/agy/resume.jsonl"),
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
            assert!(
                !events
                    .iter()
                    .any(|event| matches!(event, AgentEvent::NeedsInput { .. }))
            );
        }
        let events = replay(include_str!("../../tests/fixtures/agy/interrupt.jsonl"));
        assert!(events.iter().any(|event| matches!(event, AgentEvent::StepStarted { step } if step.kind == StepKind::Command)));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnFinished {
                outcome: TurnOutcome::Interrupted,
                ..
            })
        ));
    }
    #[test]
    fn real_plain_and_invalid_model_result() {
        let events = replay(include_str!("../../tests/fixtures/agy/plain.jsonl"));
        assert!(
            matches!(events.last(), Some(AgentEvent::TurnFinished { outcome: TurnOutcome::Completed, reply: Some(text), .. }) if text.contains("protocol-ready"))
        );
        let events = replay(include_str!("../../tests/fixtures/agy/error.jsonl"));
        assert!(
            matches!(events.last(), Some(AgentEvent::TurnFinished { outcome: TurnOutcome::Failed { message }, .. }) if message.contains("code-ui-deliberately-invalid-model"))
        );
    }
    #[test]
    fn task_list_writes_become_a_todo_step_read_from_disk() {
        let root = std::env::temp_dir().join(format!("code-ui-agy-{}", uuid::Uuid::new_v4()));
        let brain = root.join("brain/conversation");
        std::fs::create_dir_all(&brain).unwrap();
        let list = brain.join("task_list.md");
        std::fs::write(
            &list,
            "# Task List\n\n- [x] Create a\n- [/] Create b\n  * [ ] Create c\n- plain note\n",
        )
        .unwrap();
        let write = |state: &str, path: &std::path::Path| {
            json!({"event":"step_update","step_update":{"step_index":3,"step_type":"tool","state":state,
                "tool_name":"write_to_file","tool_info":{"name":"write_to_file","parameters":{"TargetFile":path}}}})
        };
        let mut parser = Parser::new("/scratch".into());
        parser.begin();
        assert!(parser.parse(&write("ACTIVE", &list)).is_empty());
        let events = parser.parse(&write("DONE", &list));
        let [
            AgentEvent::StepStarted { step },
            AgentEvent::StepFinished { .. },
        ] = events.as_slice()
        else {
            panic!("Expected a started and a finished step")
        };
        assert_eq!(step.kind, StepKind::Todo);
        assert_eq!(
            step.input["todos"],
            json!([
                {"text":"Create a","status":"completed"},
                {"text":"Create b","status":"in_progress"},
                {"text":"Create c","status":"pending"}
            ])
        );
        let notes = root.join("task_notes.md");
        std::fs::write(&notes, "- [ ] Not an agent list\n").unwrap();
        parser.begin();
        assert!(matches!(
            parser.parse(&write("ACTIVE", &notes))[..],
            [AgentEvent::StepStarted { ref step }] if step.kind == StepKind::Write
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn commentary_precedes_tools_and_repeated_updates_keep_one_step() {
        let mut parser = Parser::new("/scratch".into());
        parser.parse(&json!({"event":"step_update","step_update":{"step_index":1,"step_type":"agent_response","state":"ACTIVE","text_delta":"I will read it."}}));
        let msg = json!({"event":"step_update","step_update":{"step_index":2,"step_type":"tool","state":"ACTIVE","tool_name":"view_file","tool_info":{"parameters":{"AbsolutePath":"/scratch/README.md"}}}});
        let events = parser.parse(&msg);
        assert!(
            matches!(&events[0], AgentEvent::Commentary { text, .. } if text == "I will read it.")
        );
        assert!(matches!(&events[1], AgentEvent::StepStarted { .. }));
        assert!(parser.parse(&msg).is_empty());
        let events = parser.parse(&json!({"event":"step_update","step_update":{"step_index":2,"step_type":"tool","state":"ERROR","tool_info":{"output":"Denied"}}}));
        assert!(
            matches!(&events[0], AgentEvent::StepFinished { status: StepStatus::Failed, output: Some(text), .. } if text == "Denied")
        );
    }
}
