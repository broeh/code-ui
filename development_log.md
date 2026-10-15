# Development log

The public repository starts with the initial alpha source snapshot.

## 0.1.0-alpha.1

- Initial Linux alpha candidate with projects, multi-agent chats, context handoff, queues, file editing, activity and task lists.
- Added first-run setup, editable project/CLI preferences, optional native-history discovery and detection refresh.
- Added Ask defaults for new installs, explicit project trust, private app-data permissions and secret-answer redaction.
- Added isolated preview builds, synthetic workflow tests and release preparation tooling.
- Claude Code and Codex are primary integrations. Antigravity and Muse remain experimental.

The alpha is under active development. Consult the README and agent compatibility notes for current limitations.

## Release verification

- Added a fake Claude process check for manual mode before plan approval. All 72 local Rust tests and Clippy pass.
- Gitleaks found no secrets in the fresh repository history. Hosted RustSec checks passed with two inherited warnings documented in SECURITY.md.
- First Ubuntu run built both desktop packages and passed Rust/frontend/browser checks. Corrected asynchronous native test timing and package-content inspection before rerunning the final checks.
