# Setup and troubleshooting

## CLI not detected

Install the official CLI and authenticate in your terminal. Open Settings, enable the agent and click Check again. If shell-based detection cannot find it, choose its executable file. Paths containing spaces are supported; do not add shell quotes around a path in Settings. A leading `~/` is expanded.

Detection launches enabled executables with `--version`; it does not send a model request. A detected CLI can still require login, account access or a supported model. Use the CLI's own authentication flow. Do not paste provider tokens into CODE_UI settings or issue reports.

If the CLI is installed after CODE_UI starts, Check again refreshes detection. Disabling an agent prevents new turns from starting with it. Existing running work is not canceled by a preferences change.

## Project folders

Choose a projects root in Settings. Select Create this folder if the path does not exist. Add other folders with the folder-plus button in Projects. Creating a project creates a directory, without initializing a Git repository.

Before running an agent, trust the selected project. Hooks, plugins and configured tools may execute when a CLI starts, even before its first approval request. Trust is remembered for the canonical folder path and is separate from Ask/Full access permissions.

## Existing conversations

History discovery is off for new profiles until selected. It reads the enabled histories on disk; disabling it hides cached terminal entries without deleting provider history or imported app conversations.

Claude history respects `CLAUDE_CONFIG_DIR`; Codex history respects `CODEX_HOME`. Muse's index follows `XDG_DATA_HOME` when set. The default locations are the CLIs' conventional Linux directories. Use absolute paths for these environment overrides.

Opening a terminal conversation copies available user/assistant text and preserves its native session ID. A later send resumes the native session. A fallback reader may need to start the official CLI, so opening a native conversation can require project trust. Tool activity, images and later terminal messages are not continuously synchronized.

## Settings recovery

Malformed or unsupported settings open a recovery flow. The original file is preserved until you save; saving keeps an owner-only `settings.invalid-*.json` backup beside it. Settings writes use a temporary file and atomic rename. Repair settings before trusting a new project.

## Graphics

If WebKitGTK has rendering problems on an NVIDIA/Wayland setup, try the standard WebKit override:

```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 code-ui
```

The app no longer forces this workaround on every machine. Desktop notifications need `notify-send` and a notification service; chat badges still work when notifications are disabled or unavailable. Hyprland focus fallback is attempted only in a Hyprland session.

## Preview and rollback

Use `npm run preview:desktop` or `tools/install.sh --fast` to test the preview build. Its identifier and settings/data directories differ from the stable app. Avoid running a stable development build against a stable database merely to inspect UI changes.

Before upgrading stable, close it and copy its settings and data directories to a private backup. If reverting across a database schema change, preserve newer data separately and restore the matching database backup along with the older binary. Never replace SQLite files while the app is using them.
