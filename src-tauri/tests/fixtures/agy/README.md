# Antigravity fixtures

Recorded from the installed CLI in scratch projects. `read`, `question` and `plan` use the direct probe. Other cases use the native app. Personal paths and usernames are scrubbed; init tool inventories are removed.

Configured approvals and questions have no host response protocol. `approval` records a skipped permission request. `question` records an unnamed unknown step and the final text asking the user to reply. Neither creates an answerable native request.

`interrupt` contains the active command and the real ERROR result flushed while Stop kills the process group. There is no host interrupt input message. The adapter normalizes its own Stop as Interrupted; the native acceptance check verified that status and continuation with the same conversation ID.

The invalid-model case emits an ERROR result and also writes stderr. `error.stderr.txt` holds the relevant real error; `error.jsonl` preserves stdin/stdout traffic. The native check verified a readable Failed notice and recovery in the same conversation.

Plan passes `--mode plan` and follows the CLI's artifact review settings. It can continue into implementation automatically, as confirmed in the native check and documented in PLAN.md.
