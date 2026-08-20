# CASS Raw Session Export Review and Implementation Plan

**Prepared:** 2026-08-19  
**Target fork:** [`klittle32/coding_agent_session_search`](https://github.com/klittle32/coding_agent_session_search)  
**Upstream:** [`Dicklesworthstone/coding_agent_session_search`](https://github.com/Dicklesworthstone/coding_agent_session_search)  
**Related parser fork:** [`klittle32/franken_agent_detection`](https://github.com/klittle32/franken_agent_detection)  
**Downstream consumer:** [`klittle32/cass_memory_system`](https://github.com/klittle32/cass_memory_system)

## 1. Decision

Implement both fork issues in **one agent session**, as one shared raw-session export repair with two logical commits:

1. connector-backed raw export bridge plus Grok support and tests;
2. Letta Code support, parity tests, downstream-contract smoke tests, and documentation.

Do not split the work between parallel agents. Both defects converge on the same raw-export dispatch, normalized-message rendering, fallback behavior, and CLI end-to-end test harness. Parallel implementations would likely duplicate architecture and conflict in the same large CLI module.

Do not propose or open an upstream pull request. The work is for Kyle's fork. Preserve a clean commit history so future upstream merges remain manageable.

## 2. Review snapshot

At review time:

- fork `main`: `9ecba5c76b077ecc65f002dd8f9bc6a3e6d95979`;
- upstream `main`: `fdc648c22d9bfec3eb301154f3964e8d7d93199d`;
- the fork was five commits ahead and zero behind that upstream snapshot;
- the fork pinned `franken_agent_detection` to Kyle's fork at revision `0b04f8a2251ec775ecc23578793172976de15516`.

The fork-only change sequence was:

| Commit | Purpose |
|---|---|
| `6d00bea4` | Add Letta Code connector through FAD. |
| `aca8dfec` | Add Letta Code to TUI/HTML presentation. |
| `a9d76ecc` | Maintain the FAD fork containing the Letta connector. |
| `286211b3` | Add Prime Agent connector. |
| `9ecba5c7` | Merge current upstream while preserving Letta/Prime support. |

The implementing agent must record the actual starting SHAs before editing because either repository may have advanced since this review.

## 3. Issues reviewed

### Fork issue #1 — Grok Build exports as `UNKNOWN`

[`klittle32/coding_agent_session_search#1`](https://github.com/klittle32/coding_agent_session_search/issues/1) reports that:

- indexing and search understand Grok Build sessions;
- raw `cass export --format markdown -- <updates.jsonl>` emits one `## unknown` heading for every JSONL record;
- `cm reflect` then has no recoverable turns and produces zero playbook deltas;
- the source is an ACP event stream, not top-level `{role, content}` JSONL.

Representative event shape:

```json
{
  "timestamp": 0,
  "method": "session/update",
  "params": {
    "sessionId": "<uuid>",
    "update": {
      "sessionUpdate": "user_message_chunk",
      "content": {
        "type": "text",
        "text": "..."
      }
    }
  }
}
```

Observed methods include `session/update` and `_x.ai/session/update`. Useful record kinds include `user_message_chunk`, `agent_message_chunk`, and sometimes `agent_thought_chunk`; the stream also contains large quantities of tool/status/hook records.

### Fork issue #2 — Letta Code exports as `UNKNOWN`

[`klittle32/coding_agent_session_search#2`](https://github.com/klittle32/coding_agent_session_search/issues/2) reports that:

- indexing and search understand Letta Code `transcript.jsonl` sessions;
- raw Markdown export emits `## unknown` for all records;
- a real 6,584-record transcript contained 119 user records and 118 assistant records, but none were recovered;
- the remaining records were mainly reasoning and tool-call records that should be normalized or intentionally omitted, not rendered as thousands of unknown turns.

### Shared user-visible failure

Both issues have the same contract failure:

```text
source file -> connector/indexer: understood
source file -> cass export: generic fallback parser -> UNKNOWN
source file -> cm reflect: unusable transcript -> no useful memory deltas
```

This is not an indexing defect and should not be repaired primarily in `cm`. The canonical correction belongs in CASS's raw-session exporter so every downstream consumer receives the same normalized transcript.

## 4. Repository grounding

### 4.1 There are two export systems

The repository's `src/export.rs` handles exporting **search results** to JSON, JSONL, CSV, and HTML. It is not the raw session-file path exercised by:

```bash
cass export --format markdown -- /path/to/session-file.jsonl
```

The raw session-file exporter is wired through the large CLI implementation, historically in `src/lib.rs`. Upstream issue #20 and its fix show helper functions such as `extract_role` and `extract_text_content` performing independent JSON-shape recognition for this path.

The implementing agent must locate the current raw-export entry point by searching for all of:

```text
Conversation Export
extract_role
extract_text_content
## unknown
ImportCommand or ExportCommand handling
```

Do not modify `src/export.rs` under the assumption that it is the failing path. Shared rendering helpers may be extracted there only if that is demonstrably the cleanest fit after inspecting the current tree.

### 4.2 Index parsing is already available

CASS re-exports the FAD connector contract, including `Connector`, `ScanContext`, `NormalizedConversation`, and `NormalizedMessage`.

The relevant connectors already exist:

- `src/connectors/grok.rs` re-exports FAD's `GrokConnector`;
- `src/connectors/letta_code.rs` re-exports FAD's `LettaCodeConnector`;
- `src/connectors/mod.rs` provides connector lookup and normalized connector types.

FAD's connector interface supports:

```rust
Connector::scan(&ScanContext) -> Result<Vec<NormalizedConversation>>
```

`ScanContext` supports explicit roots. Its `with_extra_roots(...)` behavior makes those roots authoritative rather than adding default home-directory discovery. This gives the raw exporter a clean way to parse one requested session without copying connector logic.

### 4.3 Format-specific scan roots

The scan root is not identical for the two formats:

| Source passed to `cass export` | Connector | Explicit scan root |
|---|---|---|
| Grok `updates.jsonl` | `GrokConnector` | Parent session directory containing `updates.jsonl`, `summary.json`, and optional `chat_history.jsonl`. |
| Grok `chat_history.jsonl` | `GrokConnector` | Parent session directory. |
| Grok session directory | `GrokConnector` | That directory. |
| Letta `transcript.jsonl` | `LettaCodeConnector` | Exact file is supported; the containing conversation directory is also acceptable if selection remains unambiguous. |
| Letta conversation directory | `LettaCodeConnector` | That directory. |

Never take the first returned conversation without validating that it corresponds to the requested path. Normalize/canonicalize the requested path and use source provenance, external ID, or an exact-one-result assertion to prevent accidentally exporting a sibling session.

### 4.4 Prime Agent is not part of this repair

Prime Agent support explains why the fork exists and should influence the extension point, but neither reported issue establishes a Prime raw-export failure. Do not expand scope to Prime unless a newly added regression test proves the same defect. The adapter design must make a later Prime mapping straightforward without changing the public CLI contract.

## 5. Relevant upstream history and maintainer guidance

### Upstream issue #20 — historical Codex `UNKNOWN` export

[`Dicklesworthstone/coding_agent_session_search#20`](https://github.com/Dicklesworthstone/coding_agent_session_search/issues/20) is direct precedent. The raw exporter recognized only simple top-level role/content shapes while Codex stored useful fields under `payload`. Commit [`cc9250d1`](https://github.com/Dicklesworthstone/coding_agent_session_search/commit/cc9250d1d1cb7e784a33d508f5a20c7b3a90bc7d) added another format-specific branch to `extract_role` and `extract_text_content`.

That patch fixed Codex, but the Grok and Letta reports show the structural limitation of continuing to grow a second parser in the exporter.

### Upstream issue #328 — Grok's canonical parser already exists

[`Dicklesworthstone/coding_agent_session_search#328`](https://github.com/Dicklesworthstone/coding_agent_session_search/issues/328) introduced Grok ingestion. Its parser already understands ACP updates, chunk coalescing, tool events, metadata, bounds, and `chat_history.jsonl` fallback. Reimplementing only part of that behavior in `extract_role` would create two Grok parsers that can drift.

### cass-memory issue #67 — ownership belongs in CASS export

[`Dicklesworthstone/cass_memory_system#67`](https://github.com/Dicklesworthstone/cass_memory_system/issues/67) discusses Grok export failures from the `cm` side. The maintainer's resolution places native format mapping in the CASS exporter; `cm`'s direct parser is a rescue path, not the canonical implementation. Current ACP nesting also exceeds the simple fallback described there.

### Upstream issue #378 — direct parser tests miss end-to-end gates

[`Dicklesworthstone/coding_agent_session_search#378`](https://github.com/Dicklesworthstone/coding_agent_session_search/issues/378) concerns a different import/discovery defect, but the maintainer explicitly identified the testing lesson: connector tests that call `scan()` directly can pass while the documented CLI workflow remains broken. The fix must include a real `cass export` end-to-end test, not only unit tests of Grok and Letta parsers.

### Upstream issue #388 — parsing belongs at the connector boundary

[`Dicklesworthstone/coding_agent_session_search#388`](https://github.com/Dicklesworthstone/coding_agent_session_search/issues/388) describes the expected separation for Prime: parsing and normalization at the connector/FAD boundary, explicit CASS integration above it. The same boundary should be honored by raw export.

### Open-issue search result

A search of current upstream open issues for raw `cass export`, `UNKNOWN`, transcript, and cass-memory terms did not surface a directly matching open issue for these two exporter defects. The useful current guidance came from adjacent issues such as #378 and #388; #20 and #328 are the strongest historical implementation precedents.

## 6. Root cause

The raw session exporter and the connector/indexer implement separate interpretation stacks:

```text
Raw exporter:
JSON line -> generic role/content heuristics -> Markdown

Indexer:
source-specific connector -> NormalizedConversation/NormalizedMessage -> database/search
```

Grok and Letta were added to the second stack only. Therefore:

- search succeeds;
- export sees no recognized top-level role/content fields;
- every record falls through to `unknown`;
- downstream `cm reflect` receives no recoverable dialogue.

The durable fix is **normalize once, render many**: raw export should ask the appropriate connector for a normalized conversation before using the generic legacy parser.

## 7. Target design

### 7.1 Introduce a connector-backed raw-export bridge

Prefer a small dedicated module if the current raw-export logic is still embedded in `src/lib.rs`, for example:

```text
src/raw_session_export.rs
```

Keep only CLI argument handling in `src/lib.rs`. The module should expose an internal entry point conceptually similar to:

```rust
pub(crate) fn try_connector_backed_export(
    requested_path: &Path,
) -> Result<Option<NormalizedConversation>>;
```

`Ok(None)` means no supported connector confidently claims the input and the legacy generic parser may run. `Err(...)` means the input was recognized as a supported format but could not be safely normalized; do not silently convert that error into thousands of `unknown` headings.

An internal adapter enum is acceptable:

```rust
enum RawExportAdapter {
    Grok,
    LettaCode,
}
```

Do not expose a new public plugin API in this change.

### 7.2 Detection must be conservative and path-aware

Use filename plus a light content signature. Do not classify every `transcript.jsonl` or `updates.jsonl` solely by basename.

Suggested rules:

#### Grok

A source is a Grok candidate when:

- it is a directory with `updates.jsonl` or `summary.json`; or
- its basename is `updates.jsonl`, `chat_history.jsonl`, or `summary.json`; and
- the first meaningful record or sibling metadata matches Grok/ACP structure, such as:
  - `method` equal to `session/update` or `_x.ai/session/update`;
  - nested `params.update.sessionUpdate`;
  - known Grok summary/session metadata.

#### Letta Code

A source is a Letta candidate when:

- it is `transcript.jsonl` or a directory containing that file; and
- the first meaningful record contains Letta fields such as `message_type`, or a recognized `kind`/message record shape accepted by the existing connector.

The connector remains the authority on full validation. The exporter detection step only chooses which connector to ask.

### 7.3 Invoke the existing connector with an explicit root

Conceptual flow:

```rust
let connector: Box<dyn Connector> = ...;
let scan_root = adapter.scan_root(requested_path)?;
let ctx = ScanContext::local_default(data_dir, None)
    .with_extra_roots(vec![scan_root]);
let conversations = connector.scan(&ctx)?;
let conversation = select_requested_conversation(requested_path, conversations)?;
```

The exact `ScanContext` constructor must follow the current checkout. The required properties are:

- explicit roots only;
- no implicit scan of the user's entire home/session archive;
- no indexing or database mutation;
- deterministic selection of the requested source.

For Grok, pass the session directory so the connector can use `summary.json` and `chat_history.jsonl`. For Letta, pass the exact file or exact conversation directory supported by its connector.

### 7.4 Render normalized messages, not source records

Convert `NormalizedConversation` into the raw exporter's existing internal turn representation, or render it directly through one shared normalized renderer.

Required Markdown behavior:

```markdown
# Conversation Export

## user
...

## assistant
...
```

Additional canonical headings may include `reasoning` and `tool` if the existing exporter and `cm` consumer tolerate them. User and assistant turns are mandatory and must retain their normalized order.

Do not emit a Markdown heading for every raw source record. A recognized bookkeeping/status record should be:

- represented through the normalized connector output when useful; or
- intentionally omitted.

It must not become `## unknown` merely because it has no conversational role.

### 7.5 Preserve the generic fallback

The current generic parser supports many loose JSON/JSONL transcript shapes and the Codex special case. Preserve it for unrecognized inputs.

Dispatch order:

1. normalize path and inspect supported connector signatures;
2. if Grok or Letta is recognized, run that connector and render normalized output;
3. otherwise run the existing generic raw-file parser unchanged;
4. retain current unsupported-file/error behavior.

Do not route a recognized-but-malformed Grok/Letta file to the generic parser just to produce a nominally successful export. Return an actionable diagnostic naming:

- connector slug;
- requested path;
- scan root;
- whether zero or multiple conversations were produced;
- the underlying parser error, without leaking transcript content.

### 7.6 Keep parsing logic single-sourced

Do not duplicate these details in CASS raw export:

- ACP event extraction;
- chunk coalescing;
- Grok tool-call state reconstruction;
- `chat_history.jsonl` fallback/deduplication;
- Letta reasoning/tool linkage;
- Letta record validation;
- binary/base64 redaction or existing content bounds.

Those belong to the connector implementations already used by indexing.

No FAD API change appears necessary for the first implementation because both connectors can be run through `Connector::scan` with explicit roots. If the current checkout proves that assumption wrong, make the smallest reusable FAD change, test it there, pin the new immutable FAD revision in CASS, and explain why the existing public connector interface was insufficient.

## 8. Normalization policy

These tables describe the externally required result. Prefer the existing connector's behavior over a second hand-written mapping.

### 8.1 Grok

| Source event/data | Export policy |
|---|---|
| `user_message_chunk` | Coalesce as the connector does; render as `user`. |
| `agent_message_chunk` | Coalesce as the connector does; render as `assistant`. |
| `agent_thought_chunk` | Render as `reasoning` only if the normalized connector emits it and the renderer supports it; otherwise omit intentionally. Never label it `unknown`. |
| `tool_call` / useful terminal tool state | Render a bounded `tool` entry if present in normalized output. Avoid repeated status-only updates. |
| `tool_call_update` | Rely on connector reconstruction/coalescing; do not emit every update as a turn. |
| `hook_execution` and presentation/status events | Omit unless the connector intentionally emits bounded conversational content. |
| `summary.json` | Metadata/title/context, not one standalone turn per field. |
| `chat_history.jsonl` | Use connector fallback behavior when the ACP stream lacks recoverable dialogue; avoid duplicate turns. |
| Malformed trailing live-write line | Follow connector tolerance. Preserve earlier valid turns and surface a bounded warning if the current CLI contract supports warnings. |

### 8.2 Letta Code

| Source record | Export policy |
|---|---|
| `message_type=user` | Render as `user`. |
| `message_type=assistant` | Render as `assistant`. |
| reasoning records | Preserve normalized order; render as `reasoning` only when supported, otherwise omit intentionally. |
| tool call and tool result records | Preserve connector association/order; render useful bounded tool context, not internal bookkeeping. |
| linkage IDs and dates | Keep as bounded metadata only if the existing export format exposes metadata. Do not place opaque IDs in the message body merely to avoid dropping them. |
| internal/meta records | Omit unless normalized by the connector into useful context. Never emit mass `unknown` headings. |

## 9. Detailed implementation sequence

### Phase 0 — establish the baseline

1. Fetch `origin` and `upstream`.
2. Record:
   - CASS starting SHA;
   - upstream SHA merged into the fork;
   - pinned FAD repository and SHA;
   - working tree status.
3. Confirm the two issue reproductions with sanitized fixtures or newly constructed minimal fixtures.
4. Run the repository's targeted baseline tests before editing.
5. Create a local branch such as `fix/connector-backed-raw-export`.

Do not overwrite or expose Kyle's real transcripts. Fixtures must be synthetic and sanitized.

### Phase 1 — write failing CLI tests first

Add true command-level tests that execute the same path as:

```bash
cass export --format markdown -- <fixture-path>
```

The tests must initially demonstrate:

- Grok fixture: nonzero `## unknown`, missing user/assistant content;
- Letta fixture: nonzero `## unknown`, missing user/assistant content.

Prefer the repository's existing CLI test harness (`assert_cmd`, snapshots, or equivalent) rather than shelling out ad hoc from unit tests.

### Phase 2 — add the shared bridge

1. Locate the raw-export parser and renderer.
2. Extract a focused module if necessary to avoid adding more format branches to the giant CLI file.
3. Implement:
   - adapter detection;
   - explicit-root connector invocation;
   - exact conversation selection;
   - normalized-message-to-export-turn conversion;
   - recognized-format diagnostics;
   - fallback to the existing generic parser.
4. Unit-test detection and selection independently from rendering.
5. Preserve all legacy generic-export fixtures.

### Phase 3 — Grok implementation

1. Route Grok session sources through `GrokConnector`.
2. Pass the session directory, not just `updates.jsonl`, so sibling metadata/fallback files remain available.
3. Add fixtures covering:
   - `session/update`;
   - `_x.ai/session/update`;
   - adjacent chunk coalescing;
   - user and assistant text;
   - thought/tool/status noise;
   - `chat_history.jsonl` fallback;
   - malformed final line if supported by the connector contract.
4. Assert:
   - expected user/assistant text appears once and in order;
   - no `## unknown` appears for the supported fixture;
   - no duplicate fallback dialogue appears;
   - tool/status volume does not create one heading per event.

Commit this as the first logical implementation commit after tests pass.

### Phase 4 — Letta Code implementation

1. Route Letta transcript sources through `LettaCodeConnector`.
2. Use exact file or exact conversation-directory scoping.
3. Add fixtures covering:
   - user;
   - assistant;
   - reasoning;
   - tool call;
   - tool result;
   - linkage/order behavior;
   - recognized internal record that should be omitted rather than rendered unknown.
4. Assert:
   - user/assistant text appears once and in order;
   - no `## unknown` appears for the supported fixture;
   - useful normalized reasoning/tool context follows the connector's ordering policy;
   - sibling transcripts are not exported.

### Phase 5 — parity and downstream-contract tests

Add a parity helper/test for each connector:

1. scan the fixture directly with the connector and explicit `ScanContext`;
2. run the real `cass export` CLI on the same source;
3. compare the ordered user/assistant sequence after only renderer-specific whitespace normalization.

This catches future drift between indexing and export.

Add a `cm`-oriented contract smoke test without coupling CASS tests to the full `cm` implementation:

- output contains at least one `## user` and one `## assistant`;
- output is not 100% `unknown`;
- output contains the expected sentinel dialogue in order;
- output contains no raw base64/blob payload from fixture noise.

If a lightweight `cm reflect` integration test is easy to run against Kyle's local fork without introducing a cross-repository test dependency, run it manually and record the result. Do not add `cass_memory_system` as a build dependency of CASS.

### Phase 6 — regression, documentation, and cleanup

1. Run legacy raw-export tests, including the Codex shape fixed by #20.
2. Run connector tests for Grok, Letta, Prime, and Pi to prove no detection cross-talk.
3. Update help/docs only where behavior is documented. State that recognized agent-native session files are normalized through their connectors.
4. Add a concise changelog entry if the fork maintains one.
5. Run formatting, targeted tests, full tests, and baseline-compatible linting.
6. Commit Letta/parity/docs as the second logical commit.

## 10. File-level checklist

The exact tree may have changed; verify before editing.

| Area | Expected action |
|---|---|
| `src/lib.rs` | Locate raw `cass export` command handling and current generic role/content helpers. Keep CLI wiring thin. |
| New or existing raw-export module | Add adapter detection, connector invocation, conversation selection, normalized rendering, diagnostics, and generic fallback. |
| `src/connectors/mod.rs` | Reuse public connector lookup/types. Add only a small helper if necessary; do not create duplicate parser APIs. |
| `src/connectors/grok.rs` | Normally no parser change; re-exported connector should be sufficient. |
| `src/connectors/letta_code.rs` | Normally no parser change; re-exported connector should be sufficient. |
| CASS fixture directory | Add minimal sanitized Grok and Letta raw-export fixtures plus provenance/checksum entries if required by repository policy. |
| CLI/integration tests | Exercise the actual binary and exact command syntax from both issues. |
| FAD fork | Change only if explicit-root `Connector::scan` cannot safely support export. If changed, test and pin an immutable new revision. |
| `src/export.rs` | Do not assume this is the failing path. Touch only for genuinely shared rendering abstractions. |
| Documentation/changelog | Document connector-backed raw export and supported source paths. |

## 11. Required test matrix

| Test | Required assertion |
|---|---|
| Grok ACP user + assistant | Correct headings/text/order; zero unknown headings. |
| Both ACP method names | Same normalized result. |
| Grok chunk coalescing | Adjacent chunks form one logical turn according to connector behavior. |
| Grok tool/status-heavy stream | No heading per status record; bounded useful tool context only. |
| Grok chat-history fallback | Dialogue recovered once, without duplication. |
| Letta user + assistant | Correct headings/text/order; zero unknown headings. |
| Letta reasoning + tools | Connector ordering retained; no mass unknown headings. |
| Exact path scoping | Requested session only; siblings excluded. |
| Recognized malformed source | Actionable non-success or diagnostic; no silent 100% unknown success. |
| Unknown generic JSONL | Existing generic parser behavior unchanged. |
| Codex nested payload | Historical #20 behavior remains green. |
| Connector/export parity | Ordered user/assistant content matches connector normalization. |
| Large/noisy fixture | Output remains bounded by existing policies and contains no base64/blob spill. |
| Prime regression | Existing Prime indexing/integration tests remain green; no new export behavior required. |

## 12. Validation commands

Use the repository's exact documented commands if they differ. At minimum:

```bash
cargo fmt --check
cargo test <targeted_raw_export_tests>
cargo test <grok_connector_tests>
cargo test <letta_connector_tests>
cargo test
```

Run linting only to the baseline the repository currently supports, for example:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

If full lint or full test failures predate the branch, prove that with the baseline SHA and report them separately. Do not hide, weaken, or delete unrelated tests.

Manual smoke commands:

```bash
cass export --format markdown -- tests/fixtures/<grok-session>/updates.jsonl
cass export --format markdown -- tests/fixtures/<letta-session>/transcript.jsonl
```

For each output, verify:

```text
at least one ## user
at least one ## assistant
zero ## unknown for the supported fixture
expected sentinel text in source order
```

## 13. Acceptance criteria

The implementation is complete only when all of the following are true:

1. The exact reproduction in fork issue #1 produces useful Grok user/assistant Markdown rather than 100% `unknown`.
2. The exact reproduction in fork issue #2 produces useful Letta user/assistant Markdown rather than 100% `unknown`.
3. `cm reflect` can consume the resulting Markdown contract without falling into its no-recoverable-turn failure path.
4. Raw export reuses the connector normalization path; it does not contain a second ACP parser or a second Letta parser.
5. The raw exporter scans only the requested source/session and never walks the user's default archive as a side effect.
6. Recognized but malformed Grok/Letta sources fail or warn actionably instead of silently emitting a successful all-unknown transcript.
7. Legacy generic and Codex raw exports remain unchanged.
8. Grok and Letta connector/export parity tests cover the user/assistant sequence.
9. Existing Prime Agent and Letta indexing/search behavior remains green.
10. No real transcript, user path, secret, credential-bearing URL, binary payload, or base64 content is committed in fixtures or snapshots.
11. The branch contains two reviewable logical commits and no upstream PR or issue creation.

## 14. Non-goals

Do not include any of the following unless required to satisfy a failing acceptance test:

- an upstream pull request;
- a public runtime plugin system;
- a rewrite of CASS's search-result export module;
- moving all connector parsers out of CASS/FAD;
- changing `cm` to make its fallback the primary parser;
- adding Prime Agent raw export without a demonstrated failing case;
- indexing abandoned branches or redesigning connector semantics;
- changing raw session file formats;
- broad CLI output redesign unrelated to normalized role recovery.

## 15. Expected final agent report

The implementing agent should return:

1. starting and ending CASS/FAD SHAs;
2. a concise root-cause confirmation from the checked-out code;
3. files changed and why;
4. the two commit SHAs;
5. targeted and full test/lint command results;
6. before/after Grok and Letta CLI output summaries;
7. manual `cm reflect` smoke result, if run;
8. any remaining limitation, especially reasoning/tool rendering policy;
9. confirmation that no upstream PR was opened.
