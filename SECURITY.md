# Security and privacy

CODE_UI runs local coding-agent CLIs with your user account's access. Treat Full access as permission to run commands and change files without approval where supported by the CLI. Ask uses the agent's controls; it is not an additional application-wide security sandbox.

## Project trust

Before first use, CODE_UI asks you to trust the canonical project folder. CLI hooks, plugins and configured tools can execute at startup. Trust checks also protect send, retry, queued continuation and official history-reader fallback paths. Only open and run configurations you trust.

## Local storage

Conversations, drafts, tool activity and ordinary interaction answers are stored in SQLite in plain text. App data directories use owner-only permissions. Secret-marked answers and accompanying notes are redacted from retained interaction records; legacy records are sanitized on migration and snapshot reads.

Stderr is normally retained locally. Full protocol tracing is opt-in with `CODE_UI_TRACE=1`. Answer payloads are redacted from traces. Once a process presents a secret-input prompt, further trace and stderr retention is disabled for that process. This does not erase old backups, the CLIs' own history or sensitive text returned by a provider in ordinary messages.

CLI authentication is handled by the provider's unmodified executable. CODE_UI does not implement provider login or store authentication keys in its settings. Enabled child processes inherit your shell environment. Requests, usage and provider retention policies belong to the selected CLI/provider.

History discovery is optional. Disabling it does not delete previously imported conversations or provider history. Handoff can send selected conversation context to another provider, and a generated summary uses an additional outgoing-agent turn.

## Reporting

Use GitHub's private vulnerability reporting feature when enabled on the repository. Do not publish credentials, private project names, raw traces or conversation databases in an issue. If private reporting is unavailable, open a minimal issue requesting a private reporting channel without exploit details or sensitive data.

Only the current alpha branch is maintained. Dependency audit results and automated tests are useful checks, not a guarantee that the application or a provider CLI is free of vulnerabilities.

## Known upstream dependency warnings

The 2026-10-15 RustSec audit completed with two informational warnings inherited through Tauri's GTK3 stack:

- [`RUSTSEC-2024-0429`](https://rustsec.org/advisories/RUSTSEC-2024-0429.html): `glib` 0.18.5 has unsound `VariantStrIter` iterator methods that can crash optimized builds. RustSec identifies 0.20.0 or later as patched. CODE_UI does not directly use this iterator, but this is not proof that all transitive paths are unaffected. Tauri's GTK3 dependencies currently select the older GLib binding; a direct application dependency bump does not replace that stack.
- [`RUSTSEC-2024-0370`](https://rustsec.org/advisories/RUSTSEC-2024-0370.html): `proc-macro-error` 1.0.4 is unmaintained. It is used by the GTK/GLib build-time macros.

These warnings remain visible in CI. No advisory IDs are suppressed. Follow upstream GTK/Tauri compatibility changes before claiming a warning-free audit. The alpha preserves the existing dependency stack rather than introducing an unverified GTK fork.
