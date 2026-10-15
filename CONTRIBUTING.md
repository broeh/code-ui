# Contributing

This is an early Linux alpha. Keep changes focused and describe the user-visible problem, the change and the checks run.

## Local checks

Use the pinned Node and Rust toolchains and install Tauri's Linux prerequisites. Then run:

```bash
npm ci
npm test
npm run build
npm run check:rust
npx playwright install chromium
npm run test:ui
```

Browser checks own and stop their Vite server and use synthetic data. Screenshots are written under `.cache/ui-checks`. The desktop preview uses a separate application identifier and storage:

```bash
npm run preview:desktop
```

Never replace a stable installation to inspect a development build. `tools/install.sh --fast` installs an isolated preview and keeps the previous preview binary when replacing it.

Native checks need `webkit2gtk-driver`, `xvfb` and `dbus-x11` in addition to the build prerequisites:

```bash
cargo install tauri-driver --version 2.1.0 --locked
npm run tauri -- build --debug --no-bundle --config src-tauri/tauri.preview.conf.json
xvfb-run -a dbus-run-session -- python3 tools/testing/native_smoke.py src-tauri/target/debug/code-ui
```

The native check creates a temporary home, uses only a fake Codex executable and removes its process group afterwards. It verifies real IPC, project trust, Ask policy, conversation persistence and native thread resume without contacting providers.

## Protocol changes

Keep provider logic inside its adapter and emit the normalized event types. Reproduce changes with a fake process or a scrubbed scratch fixture. Optional probes in `tools/probes` drive real CLIs and can consume provider usage; they are not part of automated tests. Review every fixture before committing it.

Use each provider's public protocol documentation. Record tested versions and limitations in `docs/agents.md`. Unknown events should produce a safe fallback rather than crash a conversation. Never copy credentials, account inventories or private traces into fixtures.

## Release preparation

Linux CI runs tests, browser checks and native WebDriver checks with fake CLIs. Packaging produces AppImage and Debian artifacts on the documented baseline. Generated third-party licenses and checksums accompany release artifacts. Releases are prepared as drafts before publication.

`tools/release/export.py <empty-folder>` exports the allowlisted public source without private history or local material. Run `tools/release/audit.py <folder>` on the result before initializing its Git history. Notice generation requires installed dependencies; supplemental upstream license texts and their source URLs are recorded in `tools/release/licenses` for packages that omit them.

Keep package, Cargo and Tauri versions synchronized. Update `CHANGELOG.md` and `development_log.md` for meaningful changes. See `docs/release-plan.md` for the alpha gates and `SECURITY.md` before sharing diagnostics.
