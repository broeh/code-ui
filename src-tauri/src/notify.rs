use crate::{db::Database, env::SharedEnvironment, runtime::handoff};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};

pub struct Notifications {
    app: tauri::AppHandle,
    db: Arc<Database>,
    environment: Arc<SharedEnvironment>,
    settings: Arc<crate::settings::SettingsStore>,
    focused: AtomicBool,
    closing: AtomicBool,
}
impl Notifications {
    pub fn new(
        app: tauri::AppHandle,
        db: Arc<Database>,
        environment: Arc<SharedEnvironment>,
        settings: Arc<crate::settings::SettingsStore>,
    ) -> Arc<Self> {
        let focused = app
            .get_webview_window("main")
            .and_then(|window| window.is_focused().ok())
            .unwrap_or(false);
        Arc::new(Self {
            app,
            db,
            environment,
            settings,
            focused: AtomicBool::new(focused),
            closing: AtomicBool::new(false),
        })
    }
    pub fn focus_changed(&self, focused: bool) {
        self.focused.store(focused, Ordering::Relaxed);
        if focused && let Ok(Some(chat)) = self.db.active_chat() {
            let _ = self.db.mark_read(&chat);
            let _ = self.app.emit(
                "chat-status",
                json!({"chatId":chat,"status":self.db.status(&chat).ok(),"unread":false}),
            );
        }
    }
    pub fn close(&self) {
        self.closing.store(true, Ordering::Relaxed);
    }
    pub fn event(self: &Arc<Self>, name: &str, value: &mut Value) {
        if self.closing.load(Ordering::Relaxed) {
            return;
        }
        let Some(chat) = value["chatId"].as_str().map(str::to_owned) else {
            return;
        };
        if name == "chat-event"
            && matches!(
                value["event"]["type"].as_str(),
                Some("turnFinished" | "needsInput")
            )
        {
            let focused = self.focused.load(Ordering::Relaxed);
            let active = self.db.active_chat().ok().flatten().as_deref() == Some(&chat);
            if !focused || !active {
                let _ = self.db.set_unread(&chat, true);
            }
            if !focused
                && self.settings.get().notifications
                && let Ok((project, agent)) = self.db.notification_context(&chat)
            {
                let event = &value["event"];
                let body = notification_body(event);
                let title = format!(
                    "{} · {}",
                    std::path::Path::new(&project)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    handoff::name(agent).replace(" Code", "")
                );
                self.show(chat.clone(), title, body);
            }
        }
        if name == "chat-status" {
            value["unread"] = json!(self.db.unread(&chat).unwrap_or(false));
            let count = self.db.input_count().unwrap_or(0);
            let name = self
                .app
                .config()
                .product_name
                .as_deref()
                .unwrap_or("CODE_UI");
            let title = if count == 0 {
                name.to_owned()
            } else {
                format!("{name} ({count})")
            };
            let app = self.app.clone();
            let _ = self.app.run_on_main_thread(move || {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_title(&title);
                }
            });
        }
    }
    fn show(self: &Arc<Self>, chat: String, title: String, body: String) {
        let notifications = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(60),
                tokio::process::Command::new("notify-send")
                    .args([
                        "-a",
                        "CODE_UI",
                        "-A",
                        "default=Open",
                        "-t",
                        "10000",
                        "--",
                        &title,
                        &body,
                    ])
                    .env_clear()
                    .envs(notifications.environment.vars())
                    .kill_on_drop(true)
                    .output(),
            )
            .await;
            if let Ok(Ok(output)) = result
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.trim() == "default")
            {
                notifications.open(chat).await;
            }
        });
    }
    async fn open(&self, chat: String) {
        if self.closing.load(Ordering::Relaxed) {
            return;
        }
        let app = self.app.clone();
        let _ = self.app.run_on_main_thread(move || {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if !self.focused.load(Ordering::Relaxed)
            && self
                .environment
                .vars()
                .contains_key("HYPRLAND_INSTANCE_SIGNATURE")
        {
            let action = format!("hl.dsp.focus({{window=\"pid:{}\"}})", std::process::id());
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                tokio::process::Command::new("hyprctl")
                    .args(["dispatch", &action])
                    .env_clear()
                    .envs(self.environment.vars())
                    .kill_on_drop(true)
                    .output(),
            )
            .await;
        }
        let _ = self.app.emit("focus-chat", json!({"chatId":chat}));
    }
}
fn notification_body(event: &Value) -> String {
    let text = if event["type"] == "needsInput" {
        format!(
            "Needs your input: {}",
            event["interaction"]["questions"][0]["text"]
                .as_str()
                .or_else(|| event["interaction"]["title"].as_str())
                .unwrap_or("Open the conversation.")
        )
    } else {
        match event["outcome"]["type"].as_str() {
            Some("failed") => format!(
                "Failed: {}",
                event["outcome"]["message"]
                    .as_str()
                    .unwrap_or("Open the conversation.")
            ),
            Some("interrupted") => "Stopped.".into(),
            _ => format!(
                "Done: {}",
                event["reply"]
                    .as_str()
                    .and_then(|reply| reply.lines().next())
                    .unwrap_or("The turn finished.")
            ),
        }
    };
    text.chars().take(180).collect()
}
impl Database {
    pub fn active_chat(&self) -> Result<Option<String>, String> {
        let active = self.get_ui_state("active_tab")?;
        let tabs = self.get_ui_state("open_tabs")?;
        let tabs: Vec<Value> = tabs
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Ok(tabs
            .iter()
            .find(|tab| tab["id"].as_str() == active.as_deref())
            .and_then(|tab| tab["chatId"].as_str())
            .map(str::to_owned))
    }
    pub fn set_unread(&self, chat: &str, unread: bool) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "UPDATE chats SET unread=?2 WHERE id=?1",
                rusqlite::params![chat, unread],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn unread(&self, chat: &str) -> Result<bool, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row("SELECT unread FROM chats WHERE id=?1", [chat], |r| r.get(0))
            .map_err(|e| e.to_string())
    }
    pub fn input_count(&self) -> Result<i64, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row(
                "SELECT COUNT(*) FROM chats WHERE status='needs_input' AND hidden=0",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())
    }
    fn notification_context(
        &self,
        chat: &str,
    ) -> Result<(String, crate::agents::AgentKind), String> {
        let (project, agent): (String, String) = self
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .query_row(
                "SELECT project_path,active_agent FROM chats WHERE id=?1",
                [chat],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        Ok((
            project,
            serde_json::from_value(json!(agent)).map_err(|e| e.to_string())?,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn summaries_are_short_and_question_text_takes_priority() {
        assert_eq!(
            notification_body(
                &json!({"type":"turnFinished","outcome":{"type":"completed"},"reply":"Ready\nSecond line"})
            ),
            "Done: Ready"
        );
        assert_eq!(
            notification_body(
                &json!({"type":"needsInput","interaction":{"title":"Question","questions":[{"text":"Which color?"}]}})
            ),
            "Needs your input: Which color?"
        );
        assert_eq!(
            notification_body(&json!({"type":"turnFinished","reply":"x".repeat(400)}))
                .chars()
                .count(),
            180
        );
    }
}
