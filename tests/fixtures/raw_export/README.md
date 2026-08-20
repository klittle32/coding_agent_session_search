# Raw session export fixtures

Synthetic sanitized fixtures for `cass export --format markdown -- <path>`.
Do not copy real transcripts, local paths, IDs, or secrets here.

| Path | Purpose |
|---|---|
| `grok/session-acp/` | ACP `session/update` + `_x.ai/session/update`, chunk coalescing, thought/tool/status noise |
| `grok/session-fallback/` | `chat_history.jsonl` fallback when `updates.jsonl` has no dialogue |
| `grok/session-malformed/` | Recognized Grok stream with zero recoverable turns |
| `grok/sibling-a/` + `sibling-b/` | Exact-path scoping |
| `generic/session.jsonl` | Legacy `{role,content}` JSONL |
| `codex_nested/session.jsonl` | Historical Codex `payload.role` shape (upstream #20) |
