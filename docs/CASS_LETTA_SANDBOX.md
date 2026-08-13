# Side-by-side `cass-letta` sandbox

This fork ships a separate binary (`cass-letta`) that can index Letta Code
transcripts in addition to the agents upstream cass already supports. Homebrew
`cass` remains the daily driver until you deliberately point the fork at the
live archive.

**After reading this you should be able to** index your real local sessions
(including Letta) into a throwaway data directory, search that copy, and leave
the live Homebrew cass database, index, and daemon untouched.

## Isolation rules

The two binaries are different files. They still share the **same default data
directory** unless you override it.

| | Daily driver | Fork (sandbox) |
|---|---|---|
| Binary | Homebrew `cass` (`0.6.24`) | `~/.local/bin/cass-letta` (`0.6.24-letta.1`) |
| Data dir | `~/Library/Application Support/com.coding-agent-search.coding-agent-search` | `~/.local/share/cass-letta-sandbox` |
| SQLite | `<live>/agent_search.db` | `<sandbox>/agent_search.db` |

Indexing **reads** agent session files (Claude, Codex, Cursor, `~/.letta/transcripts`, …) and **writes** only into the data dir you give it. Session logs themselves are not rewritten.

`--data-dir` is **not** a global flag. Putting it before `robot-docs` (and several other subcommands) fails with `Could not parse arguments`. The reliable override for every subcommand is `CASS_DATA_DIR`.

If `CASS_DAEMON_SOCKET` is set in your shell, it points at the **live** daemon. Unset it for sandbox commands.

Do not run, against the live data dir, until you mean to:

- bare `cass-letta` (TUI)
- `index`, `doctor --fix`, `forget --apply`, `daemon`, `upgrade`, `models install`

`cass-letta upgrade` targets this fork’s GitHub repo, not Homebrew. Skip it while Homebrew `cass` is the daily driver.

A full sandbox ingest is a **second copy** of the corpus. The live archive on this machine was ~8.4 GB; the first sandbox run landed at ~4.8 GB. Filling the disk will hurt live cass even though the files are separate. Keep tens of GB free before indexing.

## Wrapper

Define this in the terminal you will use for the fork (or add it to `~/.zshrc`):

```bash
SANDBOX="$HOME/.local/share/cass-letta-sandbox"
mkdir -p "$SANDBOX"

cass_letta_safe() {
  env -u CASS_DAEMON_SOCKET \
    CASS_DATA_DIR="$SANDBOX" \
    CASS_SKIP_UPDATE=1 \
    CODING_AGENT_SEARCH_NO_UPDATE_PROMPT=1 \
    /Users/kyle/.local/bin/cass-letta "$@"
}
```

Keep using Homebrew `cass` in other terminals as usual.

## Verify isolation, then index

1. Snapshot the live DB so you can prove it did not move:

   ```bash
   LIVE="$HOME/Library/Application Support/com.coding-agent-search.coding-agent-search"
   stat -f 'LIVE db size=%z mtime=%Sm' "$LIVE/agent_search.db"
   df -h "$HOME/.local/share"
   ```

2. Confirm the wrapper is bound to the sandbox:

   ```bash
   cass_letta_safe robot-docs paths
   cass_letta_safe health --json
   ```

   `data dir default` / `data_dir` must be `.../cass-letta-sandbox`. A fresh
   sandbox reports `healthy: false`. That is expected (no DB yet).

3. Re-check the live snapshot (`stat` + `cass health --json`). Same size and
   mtime as step 1.

4. Lexical ingest into the sandbox only. First run on an empty sandbox is a
   full scan of every local connector, including Letta. Do not pass `--watch`
   or `--semantic` for this check.

   ```bash
   cass_letta_safe index --json
   ```

   Success looks like `"success": true`, `"data_dir"` and `"db_path"` under
   the sandbox, `"letta_code"` in `indexing_stats.connectors` / `agents_discovered`,
   and `"quarantined_conversations": 0`.

5. Search the sandbox:

   ```bash
   cass_letta_safe search "Checking Buzz conversation status" --agent letta_code --robot --limit 5 --fields summary
   cass_letta_safe search "authentication" --robot --limit 5 --fields summary
   ```

   The first query should return `"agent": "letta_code"`. The second should hit
   your usual agents (Claude, Codex, …).

6. Prove the live archive is still the snapshot from step 1:

   ```bash
   stat -f 'LIVE db size=%z mtime=%Sm' "$LIVE/agent_search.db"
   cass health --json
   ls "$SANDBOX/agent_search.db"
   ```

## First successful run (2026-08-13)

Worked example on this machine. Numbers will change; the paths and the
isolation checks should not.

- Wrapper used `CASS_DATA_DIR` only (no leading `--data-dir`).
- `cass_letta_safe index --json` exited 0 in ~126 s (`"full": false` on an empty
  sandbox; cass still rebuilt lexical via `inline_rebuild_from_scan`).
- Sandbox totals: **3118** conversations, **401761** messages, **0** quarantined.
- `letta_code`: **60** conversations, **26016** messages. There were 66
  `transcript.jsonl` files under `~/.letta/transcripts`; empty transcripts are
  not indexed.
- Also indexed: claude 1557, codex 935, pi_agent 462, grok 41, opencode 34,
  cursor 13, gemini 13, antigravity 3.
- Sandbox size after ingest: **4.8 GB** (`agent_search.db` ~2.0 GB).
- Live DB unchanged: size `2947350528`, mtime `Aug 13 09:54:57 2026`.

## Cleanup

Deleting `~/.local/share/cass-letta-sandbox` removes only the copy. Do not
delete anything under the Application Support cass directory.

## When you are ready to stop sandboxing

Pointing `cass-letta` at the live data dir is a deliberate switch, not the
default. Do that only after you are comfortable with the fork. Until then,
Homebrew `cass` stays the daily driver and every fork command goes through
`cass_letta_safe`.
