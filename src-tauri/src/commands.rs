use crate::{AppState, env::AgentStatus};
use tauri::State;

#[tauri::command]
pub async fn agent_status(state: State<'_, AppState>) -> Result<Vec<AgentStatus>, String> {
    Ok(state.environment.agents())
}

#[tauri::command]
pub async fn get_settings(
    state: State<'_, AppState>,
) -> Result<crate::settings::SettingsSnapshot, String> {
    Ok(state.settings.snapshot())
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: crate::settings::Settings,
    create_root: bool,
) -> Result<crate::settings::SettingsSnapshot, String> {
    let _guard = state.configuration_gate.lock().await;
    let previous_permission = state.settings.get().default_permission;
    let permission = settings.default_permission;
    state.settings.save(settings, create_root)?;
    if previous_permission != permission {
        for mut defaults in state.db.agent_defaults()? {
            defaults.permission = permission;
            state.db.set_agent_defaults(&defaults)?;
        }
    }
    let environment = crate::env::Environment::capture(&state.settings.get()).await;
    state.environment.replace(environment);
    state.history.request_scan(None);
    Ok(state.settings.snapshot())
}

#[tauri::command]
pub async fn refresh_agents(
    state: State<'_, AppState>,
    settings: Option<crate::settings::Settings>,
) -> Result<Vec<AgentStatus>, String> {
    let _guard = state.configuration_gate.lock().await;
    let persist = settings.is_none();
    let environment =
        crate::env::Environment::capture(&settings.unwrap_or_else(|| state.settings.get())).await;
    let agents = environment.agents.clone();
    if persist {
        state.environment.replace(environment);
    }
    Ok(agents)
}

#[tauri::command]
pub async fn trust_project(
    state: State<'_, AppState>,
    project: String,
) -> Result<crate::settings::SettingsSnapshot, String> {
    state.settings.trust(&project)?;
    Ok(state.settings.snapshot())
}

fn require_chat_trusted(state: &AppState, chat: &str) -> Result<(), String> {
    let snapshot = state.db.snapshot(chat)?;
    state
        .settings
        .require_trusted(snapshot["project"].as_str().ok_or("Chat has no project")?)
}

#[tauri::command]
pub async fn get_ui_state(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    state.db.get_ui_state(&key)
}

#[tauri::command]
pub async fn set_ui_state(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.db.set_ui_state(&key, &value)
}

#[tauri::command]
pub async fn list_chats(
    state: State<'_, AppState>,
    project: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let mut chats = state.db.list_app_chats(project.as_deref())?;
    let history_agents = state.settings.get().history_agents;
    chats.extend(
        state
            .db
            .list_native(project.as_deref())?
            .into_iter()
            .filter(|chat| {
                history_agents
                    .iter()
                    .any(|agent| chat["agent"] == agent.as_str())
            }),
    );
    chats.sort_by_key(|chat| std::cmp::Reverse(chat["updatedAt"].as_i64().unwrap_or(0)));
    Ok(chats)
}
#[tauri::command]
pub async fn open_chat(
    state: State<'_, AppState>,
    chat_id: String,
) -> Result<serde_json::Value, String> {
    state.db.snapshot(&chat_id)
}
#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    target: crate::runtime::SendTarget,
    text: String,
    settings: crate::agents::TurnSettings,
    handoff_mode: Option<crate::runtime::handoff::HandoffMode>,
) -> Result<serde_json::Value, String> {
    let project = if let Some(chat) = &target.chat_id {
        state.db.snapshot(chat)?["project"]
            .as_str()
            .ok_or("Chat has no project")?
            .to_owned()
    } else {
        target
            .draft
            .as_ref()
            .ok_or("Choose a project first")?
            .project
            .clone()
    };
    state.settings.require_trusted(&project)?;
    state
        .runtime
        .send_with_handoff(target, text, settings, handoff_mode)
        .await
}
#[tauri::command]
pub async fn stop(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    state.runtime.stop(&chat_id).await
}
#[tauri::command]
pub async fn retry(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    require_chat_trusted(&state, &chat_id)?;
    state.runtime.retry(&chat_id).await
}
#[tauri::command]
pub async fn resume_queue(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    require_chat_trusted(&state, &chat_id)?;
    state.runtime.resume_queue(&chat_id).await
}
#[tauri::command]
pub async fn respond_interaction(
    state: State<'_, AppState>,
    chat_id: String,
    interaction_id: String,
    response: crate::agents::InteractionResponse,
) -> Result<(), String> {
    state
        .runtime
        .respond(&chat_id, interaction_id, response)
        .await
}
#[tauri::command]
pub async fn unqueue(
    state: State<'_, AppState>,
    chat_id: String,
    pos: i64,
) -> Result<String, String> {
    state.runtime.unqueue(&chat_id, pos).await
}

#[tauri::command]
pub async fn list_projects(
    state: State<'_, AppState>,
) -> Result<Vec<crate::projects::ProjectRow>, String> {
    crate::projects::list(
        &state.db,
        std::path::Path::new(&state.settings.get().projects_root),
    )
}
#[tauri::command]
pub async fn create_project(state: State<'_, AppState>, name: String) -> Result<String, String> {
    crate::projects::create(
        &state.db,
        std::path::Path::new(&state.settings.get().projects_root),
        &name,
    )
}
#[tauri::command]
pub async fn add_folder(state: State<'_, AppState>, path: String) -> Result<String, String> {
    crate::projects::add(&state.db, &path)
}
#[tauri::command]
pub async fn set_pinned(
    state: State<'_, AppState>,
    path: String,
    pinned: bool,
) -> Result<(), String> {
    crate::projects::set_pinned(&state.db, &path, pinned)
}
#[tauri::command]
pub async fn hide_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    crate::projects::hide(&state.db, &path)
}
#[tauri::command]
pub async fn touch_project(state: State<'_, AppState>, path: String) -> Result<(), String> {
    crate::projects::touch(&state.db, &path)
}
#[tauri::command]
pub async fn rename_chat(
    state: State<'_, AppState>,
    chat_id: String,
    title: String,
) -> Result<(), String> {
    state.db.rename_chat(&chat_id, &title)
}
#[tauri::command]
pub async fn hide_chat(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    state.db.hide_chat(&chat_id)
}
#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    state.db.mark_read(&chat_id)
}
#[tauri::command]
pub async fn close_chat(state: State<'_, AppState>, chat_id: String) -> Result<(), String> {
    state.runtime.close(&chat_id).await
}
#[tauri::command]
pub async fn get_agent_defaults(
    state: State<'_, AppState>,
) -> Result<Vec<crate::agents::TurnSettings>, String> {
    state.db.agent_defaults()
}
#[tauri::command]
pub async fn set_agent_defaults(
    state: State<'_, AppState>,
    settings: crate::agents::TurnSettings,
) -> Result<(), String> {
    state.db.set_agent_defaults(&settings)
}
#[tauri::command]
pub async fn list_models(
    state: State<'_, AppState>,
    agent: crate::agents::AgentKind,
) -> Result<Vec<serde_json::Value>, String> {
    let cwd = crate::paths::data_dir()?.join("catalog");
    crate::privacy::private_dir(&cwd).map_err(|e| e.to_string())?;
    let binary = state
        .environment
        .binary(agent.as_str())
        .ok_or("This agent is disabled or not installed. Check Settings.")?;
    match agent {
        crate::agents::AgentKind::Codex => {
            return crate::agents::codex::models(binary, state.environment.vars(), cwd).await;
        }
        crate::agents::AgentKind::Agy => {
            return crate::agents::agy::models(&binary, &state.environment.vars(), &cwd).await;
        }
        crate::agents::AgentKind::Muse => {
            return crate::agents::muse::models(binary, state.environment.vars(), cwd).await;
        }
        crate::agents::AgentKind::Claude => {}
    }
    if agent != crate::agents::AgentKind::Claude {
        return Ok(Vec::new());
    }
    Ok(["fable","opus","sonnet","haiku"].into_iter().map(|id|serde_json::json!({"id":id,"label":id,"efforts":["low","medium","high","xhigh","max"]})).collect())
}
#[tauri::command]
pub async fn handoff_preview(
    state: State<'_, AppState>,
    chat_id: String,
    target_agent: crate::agents::AgentKind,
) -> Result<serde_json::Value, String> {
    state.runtime.preview(&chat_id, target_agent)
}

#[tauri::command]
pub async fn open_external(app: tauri::AppHandle, abs_path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(abs_path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_dir(
    project: String,
    rel: String,
    show_ignored: bool,
) -> Result<Vec<crate::files::tree::Entry>, String> {
    crate::files::tree::list(&project, &rel, show_ignored)
}
#[tauri::command]
pub async fn list_all_files(
    state: State<'_, AppState>,
    project: String,
) -> Result<Vec<String>, String> {
    state.files.all(&project)
}
#[tauri::command]
pub async fn read_text_file(project: String, rel: String) -> Result<serde_json::Value, String> {
    crate::files::ops::read_text(&project, &rel)
}
#[tauri::command]
pub async fn write_text_file(
    state: State<'_, AppState>,
    project: String,
    rel: String,
    text: String,
    expected_mtime: Option<String>,
) -> Result<serde_json::Value, String> {
    let result = crate::files::ops::write_text(&project, &rel, &text, expected_mtime.as_deref())?;
    state.files.invalidate(&project);
    Ok(result)
}
#[tauri::command]
pub async fn read_image_file(project: String, rel: String) -> Result<String, String> {
    use base64::Engine;
    let path = crate::files::ops::existing(&project, &rel)?;
    let mime = match path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return Err("Not an image.".into()),
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD
            .encode(std::fs::read(path).map_err(|e| e.to_string())?)
    ))
}
#[tauri::command]
pub async fn create_file(
    state: State<'_, AppState>,
    project: String,
    rel_dir: String,
    name: String,
) -> Result<String, String> {
    let rel = crate::files::ops::create(&project, &rel_dir, &name, false)?;
    state.files.invalidate(&project);
    Ok(rel)
}
#[tauri::command]
pub async fn create_folder(
    state: State<'_, AppState>,
    project: String,
    rel_dir: String,
    name: String,
) -> Result<String, String> {
    let rel = crate::files::ops::create(&project, &rel_dir, &name, true)?;
    state.files.invalidate(&project);
    Ok(rel)
}
#[tauri::command]
pub async fn rename_path(
    state: State<'_, AppState>,
    project: String,
    from: String,
    to: String,
) -> Result<(), String> {
    crate::files::ops::rename(&project, &from, &to)?;
    state.files.invalidate(&project);
    Ok(())
}
#[tauri::command]
pub async fn trash_path(
    state: State<'_, AppState>,
    project: String,
    rel: String,
) -> Result<(), String> {
    crate::files::ops::trash_path(&project, &rel)?;
    state.files.invalidate(&project);
    Ok(())
}
#[tauri::command]
pub async fn reveal(app: tauri::AppHandle, abs_path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(abs_path)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn watch_project(state: State<'_, AppState>, project: String) -> Result<(), String> {
    state.files.watch(&project)
}

#[tauri::command]
pub async fn scan_native_sessions(
    state: State<'_, AppState>,
    project: Option<String>,
) -> Result<(), String> {
    state.history.request_scan(project);
    Ok(())
}
#[tauri::command]
pub async fn open_native_session(
    state: State<'_, AppState>,
    agent: crate::agents::AgentKind,
    native_id: String,
) -> Result<serde_json::Value, String> {
    let id = state.history.open(agent, &native_id).await?;
    state.db.snapshot(&id)
}
