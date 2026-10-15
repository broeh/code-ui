# Muse MSP fixtures

Recorded from the installed CLI and native app in scratch projects, then scrubbed. Standard cases are plain, read, edit, question, approval, interrupt, error and resume. The extra cancellation case also contains recovery of an intentionally rejected answer.

Every frame uses newline JSON-RPC 2.0. `initialize` is followed by `initialized`. Approval and question requests receive an empty presentation receipt; actual responses travel as separate commands with fresh UUIDv7 commandIds.

Approval choice IDs, labels and the current requirement object come from the native request. No persistent policy changes are made by the probe. The recorded approval probe rejects once, and the native app checks both Reject and Allow once.

The installed runtime requires a reason for `userInput/cancel`, although its exported stable schema marks that field optional. The adapter sends one. Rejected answers restore the pending card so it remains answerable. Free-text and note fields use the native 500-character limit.

The invalid-model selection is admitted, then fails the turn with `turn/completed.terminal = failed`. It is not fabricated as a JSON-RPC error. Resume keeps the original native session and its context.
