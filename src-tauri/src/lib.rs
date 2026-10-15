pub mod agents;
pub mod commands;
pub mod db;
pub mod env;
pub mod files;
pub mod history;
pub mod notify;
pub mod paths;
pub mod privacy;
pub mod projects;
pub mod runtime;
pub mod settings;
use std::sync::Arc;
use tauri::{Emitter, Manager};

pub struct AppState {
    pub settings: Arc<settings::SettingsStore>,
    pub configuration_gate: tokio::sync::Mutex<()>,
    pub environment: Arc<env::SharedEnvironment>,
    pub db: Arc<db::Database>,
    pub runtime: Arc<runtime::Runtime>,
    pub files: files::Files,
    pub history: Arc<history::History>,
    pub notifications: Arc<notify::Notifications>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            paths::initialize(&app.config().identifier)?;
            let settings = Arc::new(settings::SettingsStore::load(
                paths::config_dir()?.join("settings.json"),
            ));
            let data_dir = paths::data_dir()?;
            privacy::private_dir(&data_dir)?;
            let db = Arc::new(db::Database::open(&data_dir.join("code-ui.db"))?);
            let environment = Arc::new(env::SharedEnvironment::from(
                tauri::async_runtime::block_on(env::Environment::capture(&settings.get())),
            ));
            let notifications = notify::Notifications::new(
                app.handle().clone(),
                db.clone(),
                environment.clone(),
                settings.clone(),
            );
            let alerts = notifications.clone();
            let handle = app.handle().clone();
            let runtime = runtime::Runtime::new(
                db.clone(),
                environment.clone(),
                data_dir.join("logs"),
                Arc::new(move |name, mut value| {
                    alerts.event(name, &mut value);
                    let _ = handle.emit(name, value);
                }),
            );
            let handle = app.handle().clone();
            let files = files::Files::new(Arc::new(move |name, value| {
                let _ = handle.emit(name, value);
            }));
            let handle = app.handle().clone();
            let history = history::History::new(
                db.clone(),
                dirs::home_dir().ok_or("No home directory")?,
                environment.clone(),
                settings.clone(),
                Arc::new(move |name, value| {
                    let _ = handle.emit(name, value);
                }),
            );
            history.request_scan(None);
            app.manage(AppState {
                settings,
                configuration_gate: tokio::sync::Mutex::new(()),
                environment,
                db,
                runtime,
                files,
                history,
                notifications,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event
                && let Some(state) = window.app_handle().try_state::<AppState>()
            {
                state.notifications.focus_changed(*focused);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent_status,
            commands::get_settings,
            commands::save_settings,
            commands::refresh_agents,
            commands::trust_project,
            commands::get_ui_state,
            commands::set_ui_state,
            commands::send_message,
            commands::open_chat,
            commands::open_native_session,
            commands::scan_native_sessions,
            commands::list_chats,
            commands::stop,
            commands::retry,
            commands::resume_queue,
            commands::respond_interaction,
            commands::unqueue,
            commands::list_projects,
            commands::create_project,
            commands::add_folder,
            commands::set_pinned,
            commands::hide_project,
            commands::touch_project,
            commands::rename_chat,
            commands::hide_chat,
            commands::mark_read,
            commands::close_chat,
            commands::get_agent_defaults,
            commands::set_agent_defaults,
            commands::list_models,
            commands::handoff_preview,
            commands::open_external,
            commands::list_dir,
            commands::list_all_files,
            commands::read_text_file,
            commands::write_text_file,
            commands::read_image_file,
            commands::create_file,
            commands::create_folder,
            commands::rename_path,
            commands::trash_path,
            commands::reveal,
            commands::watch_project
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
                let state = app.state::<AppState>();
                state.notifications.close();
                tauri::async_runtime::block_on(state.runtime.shutdown());
            }
        });
}
