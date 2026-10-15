# Agent compatibility

The application adapts four different stdio protocols into the same event stream. Enabling a CLI does not mean every feature is available. Authentication and provider access remain the CLI's responsibility.

| Integration | Protocol baseline recorded during development | Ask | Plan | Models |
| --- | --- | --- | --- | --- |
| Claude Code | 2.1.296; task tools additionally checked with 2.1.300 | Native approval cards | Approval enters manual mode | Default plus configured aliases |
| Codex | 0.162.1 | Workspace-write sandbox with approval requests | Unavailable in this app | App Server catalog |
| Antigravity | 1.3.3 | Uses CLI configuration; unanswerable approval requests may be skipped or fail | May continue into implementation under CLI policy | CLI catalog |
| Muse | 1.4.6-R6000.1 | Native MSP approval cards | Unavailable in this app | MSP catalog |

These are recorded compatibility baselines, not a guarantee for arbitrary newer or older versions. Automated checks replay synthetic/scrubbed protocol fixtures and use fake processes; they do not consume provider usage. Validate real CLI updates in scratch projects before changing the compatibility table.

## Permissions

New installations default to Ask. Full access disables the available approval/sandbox controls where the adapter supports it. A plan approval does not grant full access. Existing explicit session permissions and legacy installation defaults are preserved.

Codex Ask uses workspace-write with network access disabled and requests escalations from the user. The app cannot handle MCP elicitation yet and declines those requests with a visible explanation.

Antigravity's configured mode cannot present answerable approval cards in the current print protocol. Its Plan option is not a promise that files will remain unchanged. The UI explains these differences.

## Models and effort

Keep Default when a specific model is unavailable. Claude aliases and effort options are version-dependent. Codex, Antigravity and Muse use their catalogs, though account access can still change between discovery and a turn. Catalog discovery uses an owned scratch folder rather than executing inside an untrusted project.

Claude's task-list adapter currently enables the CLI's TodoWrite compatibility switches only for its child process. Codex question support currently enables the recorded `default_mode_request_user_input` feature for its child process. These are version-sensitive integration points and do not change the user's global CLI configuration.

## Public protocol references

- [Claude programmatic use](https://code.claude.com/docs/en/headless)
- [Codex App Server](https://learn.chatgpt.com/docs/app-server)
- [Antigravity headless mode](https://antigravity.google/docs/cli/headless/)
- [Muse Session Protocol](https://meta-models.github.io/muse-code-sdk/next/guides/msp-concepts/)
