# Protocol probes

Small scripts that drive the real agent CLIs and print the protocol traffic.
They were used on 2026-10-12 to verify the adapter behavior documented in `docs/agents.md`.
Use them to re-check a CLI after it updates, and as a starting point for
recording test fixtures.

Run them from a scratch folder, never from a real project.

```bash
# Claude Code: SDK-style host, answers AskUserQuestion with "Blue".
# Args: <cwd> <permission-mode> <1 = add --permission-prompt-tool stdio>
python3 -I tools/probes/claude_probe.py /tmp/probe bypassPermissions 1

# Codex app-server: initialize, thread/start, turn/start, declines approvals.
python3 -I tools/probes/codex_probe.py /tmp/probe
```

The Claude probe uses `--model haiku --no-session-persistence`, and the
Codex probe uses `ephemeral: true`, so they leave no saved sessions. The app
itself must never use those two options.

Antigravity and Muse probes are included below. These are optional contributor tools, not application setup steps. Most probes need Python 3.10 or later; `muse_probe.py` currently needs Python 3.14 for `uuid.uuid7`.

Probes use real provider accounts, may consume paid usage, and may save scratch sessions. Automated app tests use synthetic fixtures and fake CLIs instead.

Record a complete Codex scratch trace with the current protocol:

```bash
python3 -I tools/probes/codex_trace_probe.py /tmp/codex-scratch /tmp/codex-trace.jsonl --case question
python3 -I tools/probes/scrub_trace.py /tmp/codex-trace.jsonl src-tauri/tests/fixtures/codex/question.jsonl --agent codex --scratch /tmp/codex-scratch
```

Cases are plain, read, edit, question and approval. The installed Codex 0.162.1 exposes `default_mode_request_user_input` through `codex features list`; the probe and app enable it for their child process so normal chat can ask questions. This does not change the user's CLI configuration. The app preserves native threads; only the probe uses ephemeral threads. Review scrubbed files before committing.

Antigravity probes use the installed model list's Flash Low model and its saved conversations:

```bash
python3 -I tools/probes/agy_probe.py /tmp/agy-scratch /tmp/agy-trace.jsonl --case question
python3 -I tools/probes/scrub_trace.py /tmp/agy-trace.jsonl src-tauri/tests/fixtures/agy/question.jsonl --agent agy --scratch /tmp/agy-scratch
```

Cases are plain, read, plan and question. In the installed print protocol, `ask_question` produces an unnamed unknown step with no question parameters and skips selection. The final reply asks for a normal text answer. There is no host receipt or approval/question answer message to send.

Muse MSP probe, using the installed CLI login and Spark model:

```bash
python3 -I tools/probes/muse_probe.py /tmp/muse-scratch /tmp/muse-trace.jsonl --case question
python3 -I tools/probes/scrub_trace.py /tmp/muse-trace.jsonl src-tauri/tests/fixtures/muse/question.jsonl --agent muse --scratch /tmp/muse-scratch
```

Cases are plain, read, edit, question and approval. Server requests first receive an empty JSON-RPC presentation receipt. Decisions and answers are separate commands with fresh UUIDv7 commandIds. The approval probe selects a once-only rejection; it does not create persistent permission rules. Probes keep durable scratch sessions, as the app does, so resume behavior can be checked.
