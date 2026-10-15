pub mod agy;
pub mod claude;
pub mod codex;
pub mod jsonrpc;
pub mod muse;
pub mod oneliner;
pub mod process;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    Agy,
    Muse,
}
impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Agy => "agy",
            Self::Muse => "muse",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    Yolo,
    Ask,
    Plan,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSettings {
    pub agent: AgentKind,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission: Permission,
}
impl Default for TurnSettings {
    fn default() -> Self {
        Self {
            agent: AgentKind::Claude,
            model: None,
            effort: None,
            permission: Permission::Ask,
        }
    }
}
#[derive(Debug)]
pub enum AgentCommand {
    Send {
        turn_id: String,
        text: String,
        settings: TurnSettings,
    },
    Respond {
        interaction_id: String,
        response: InteractionResponse,
    },
    Interrupt,
    Shutdown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentEvent {
    SessionReady {
        native_id: String,
        model: Option<String>,
    },
    TurnStarted {
        turn_id: String,
    },
    StepStarted {
        step: Step,
    },
    StepUpdated {
        id: String,
        title: Option<String>,
        output_append: Option<String>,
    },
    StepFinished {
        id: String,
        status: StepStatus,
        output: Option<String>,
    },
    Commentary {
        id: String,
        text: String,
    },
    Usage {
        used_tokens: u64,
        window_tokens: Option<u64>,
    },
    ReplyDelta {
        text: String,
    },
    NeedsInput {
        interaction: Interaction,
    },
    InteractionClosed {
        id: String,
    },
    InteractionRejected {
        id: String,
        message: String,
    },
    TurnFinished {
        outcome: TurnOutcome,
        reply: Option<String>,
        duration_ms: u64,
    },
    Error {
        message: String,
    },
    Exited {
        code: Option<i32>,
        stderr_tail: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: StepKind,
    pub title: String,
    pub input: Value,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepKind {
    Thinking,
    Read,
    Search,
    List,
    Edit,
    Write,
    Command,
    Web,
    Subagent,
    Todo,
    Mcp,
    Skill,
    Compact,
    Other,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Running,
    Done,
    Failed,
    Declined,
    Interrupted,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TurnOutcome {
    Completed,
    Failed { message: String },
    Interrupted,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interaction {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub questions: Vec<Question>,
    pub approval: Option<Approval>,
    pub plan: Option<String>,
    #[serde(default = "default_true")]
    pub can_dismiss: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_text_length: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub header: String,
    pub text: String,
    pub options: Vec<QOption>,
    pub multi: bool,
    pub allow_other: bool,
    #[serde(default)]
    pub secret: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QOption {
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Approval {
    pub summary: String,
    pub detail: String,
    pub choices: Vec<Choice>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub question_id: String,
    pub labels: Vec<String>,
    pub other: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InteractionResponse {
    Answers {
        answers: Vec<Answer>,
        note: Option<String>,
    },
    Choice {
        choice_id: String,
        feedback: Option<String>,
    },
    Dismiss,
}

fn default_true() -> bool {
    true
}
pub struct StartContext {
    pub cwd: std::path::PathBuf,
    pub env: std::collections::HashMap<String, String>,
    pub binary: String,
    pub native_id: Option<String>,
    pub chat_id: String,
    pub settings: TurnSettings,
    pub log_dir: std::path::PathBuf,
}
pub fn spawn_adapter(
    context: StartContext,
) -> (
    tokio::sync::mpsc::Sender<AgentCommand>,
    tokio::sync::mpsc::Receiver<AgentEvent>,
) {
    let (commands, rx) = tokio::sync::mpsc::channel(32);
    let (tx, events) = tokio::sync::mpsc::channel(256);
    match context.settings.agent {
        AgentKind::Codex => {
            tokio::spawn(codex::run(context, rx, tx));
        }
        AgentKind::Agy => {
            tokio::spawn(agy::run(context, rx, tx));
        }
        AgentKind::Muse => {
            tokio::spawn(muse::run(context, rx, tx));
        }
        AgentKind::Claude => {
            tokio::spawn(claude::run(context, rx, tx));
        }
    }
    (commands, events)
}
pub fn cap_output(text: &str) -> String {
    let mut end = text.len().min(16_384);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}
