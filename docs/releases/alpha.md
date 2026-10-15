Initial Linux x86-64 alpha of CODE_UI, a desktop workspace for separately installed coding-agent CLIs.

- First-run setup for project folders, optional CLIs and preferences.
- Claude Code and Codex integrations, with experimental Antigravity and Muse support.
- Local conversations, project/file browsing, approval cards and context handoffs.
- Ask by default, explicit project trust and opt-in native history discovery.

Provider CLIs and authentication are not bundled. Install and sign in through the official CLI before enabling it. Detection checks installation and version, not account readiness. Provider usage limits and charges apply.

Built on Ubuntu 22.04. Download the `.deb` or `.AppImage` and verify it with `SHA256SUMS`. Linux distributions and graphics stacks vary. Read the README and setup guide before use. These artifacts are unsigned; automatic updates are not included.

License texts accompany the release and are bundled in the app. AppImage system-library copyright notices are preserved, and `SYSTEM_LIBRARY_SOURCES.json` identifies their exact Ubuntu source versions and source download pages. See `THIRD_PARTY_NOTICES.md` for inspection and redistribution details.

This is an early public alpha. Keep backups of important projects and local conversations. CLI protocol compatibility can change with provider updates. macOS and Windows are not supported by this release.

The RustSec check passes with two informational warnings inherited from Tauri's GTK3 dependencies: GLib's `VariantStrIter` soundness issue and the unmaintained `proc-macro-error` package. See `SECURITY.md` for scope and advisory links. These warnings are not suppressed.
