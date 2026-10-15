# Architecture

CODE_UI uses Tauri 2 with a Rust backend and a React/TypeScript frontend. The backend owns processes, protocols, persistence and file access; the frontend owns rendering and interaction.

## Main paths

- `src/components`: projects, chats, tabs, composer, file editor, activity, task lists and setup/preferences.
- `src/store`: workspace/tab and file-editor state, including ownership of asynchronous sends and edits.
- `src/ipc`: typed command/event contracts and the synthetic browser preview.
- `src-tauri/src/commands.rs`: Tauri commands and project-trust checks.
- `src-tauri/src/runtime`: serialized chat actors, message queues, lifecycle and handoff logic.
- `src-tauri/src/agents`: protocol adapters and a shared child-process transport.
- `src-tauri/src/db`: SQLite migrations, message/session state and restart recovery.
- `src-tauri/src/files`: canonical project paths, safe file operations, directory traversal and watching.
- `src-tauri/src/history`: optional native-session discovery, readers and official CLI fallback readers.
- `src-tauri/src/settings.rs`: versioned settings, atomic writes, legacy migration and recovery.
- `src-tauri/src/paths.rs`: stable/preview namespaces selected by the compiled application identifier.
- `src-tauri/src/privacy.rs`: owner-only storage, secret-answer redaction and diagnostic filtering.

## Lifecycle

One chat actor serializes sends, responses, stops and queued messages. Each participating agent has its own adapter/process and native session ID. Adapters emit normalized events; the runtime persists them and sends frontend events. Returning to an agent resumes its native session, while the selected handoff supplies conversation context it missed.

Child processes use separate Unix process groups. Interrupt is attempted through the protocol when supported; termination cleans up the group and descendants. Idle adapters are reaped and resume from the saved session ID later. SQLite recovery marks unfinished work interrupted when the app starts again.

File operations canonicalize paths, reject traversal outside the project and check external edits before replacing text. These protections apply to the app's editor; agent execution permissions are separate and depend on the selected CLI mode.

Live preferences update future process discovery without replacing running sessions. New project execution is checked for trust in the UI and backend. Preview builds isolate configuration, database, logs and single-instance identity from stable builds.
