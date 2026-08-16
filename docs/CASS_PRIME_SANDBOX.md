# Side-by-side `cass-prime` sandbox

This fork ships a separate binary (`cass-prime`) that can index Prime Agent
sessions **and** Letta Code transcripts, in addition to the agents upstream
cass already supports.

**2026-08-16:** the live daily driver on this machine is now `cass-prime`
(via `~/.local/bin/cass`). Homebrew `cass` was uninstalled after that
cutover. See [CASS_PRIME_CUTOVER.md](CASS_PRIME_CUTOVER.md). This sandbox
runbook is still the way to test a new fork build without writing the live
archive.

**After reading this you should be able to** install `cass-prime` next to
Homebrew `cass` and `cass-letta`, index your real local sessions (including
Prime) into a throwaway data directory, search that copy, and leave the live
Homebrew cass database, index, daemon, and the existing Letta sandbox
untouched.

## Isolation rules

The three binaries are different files. They still share the **same default
data directory** unless you override it.

Pre-cutover layout (this is still the right isolation map for a *new*
fork build). After 2026-08-16 the live column is served by `cass-prime`
via the `~/.local/bin/cass` shim; Homebrew `cass` is gone.

| | Live archive | Letta sandbox (existing) | Prime sandbox (this check) |
|---|---|---|---|
| Binary (pre-cutover) | Homebrew `cass` (`0.6.24`) | `~/.local/bin/cass-letta` (`0.6.24-letta.1`) | `~/.local/bin/cass-prime` (`0.6.25-letta-prime.1`) |
| Binary (after cutover) | `~/.local/bin/cass` → `cass-prime` | unchanged | unchanged |
| Data dir | `~/Library/Application Support/com.coding-agent-search.coding-agent-search` | `~/.local/share/cass-letta-sandbox` | `~/.local/share/cass-prime-sandbox` |
| SQLite | `<live>/agent_search.db` | `<letta-sandbox>/agent_search.db` | `<prime-sandbox>/agent_search.db` |

Do **not** reuse the Letta sandbox directory. That copy is a Letta-era
checkpoint. Prime verification needs its own empty data dir.

Indexing **reads** agent session files (Claude, Codex, Cursor,
`~/.letta/transcripts`, `~/.prime/agent/sessions`, …) and **writes** only
into the data dir you give it. Session logs themselves are not rewritten.

`--data-dir` is **not** a global flag. Putting it before `robot-docs` (and
several other subcommands) fails with `Could not parse arguments`. The
reliable override for every subcommand is `CASS_DATA_DIR`.

If `CASS_DAEMON_SOCKET` is set in your shell, it points at the **live**
daemon. This machine currently has
`CASS_DAEMON_SOCKET=/Users/kyle/Library/Caches/cass/daemon.sock`. Unset it
for sandbox commands.

Do not run, against the live data dir, until you mean to:

- bare `cass-prime` (TUI)
- `index`, `doctor --fix`, `forget --apply`, `daemon`, `upgrade`, `models install`

`cass-prime upgrade` targets this fork’s GitHub repo
(`klittle32/coding_agent_session_search`), not Homebrew. Skip it while
Homebrew `cass` is the daily driver.

A full sandbox ingest is a **second copy** of the corpus. The Letta sandbox
landed at ~4.8 GB; expect a similar Prime sandbox. Filling the disk will
hurt live cass even though the files are separate. Keep tens of GB free
before indexing.

## Install the binary (do not replace Homebrew `cass`)

The tagged release binary from the integration run lives at:

```text
/tmp/cass-prime-release-target/release/cass
```

That path is a temp build. Copy it to a stable name **next to** `cass-letta`,
not over Homebrew `cass` and not over `cass-letta`:

```bash
mkdir -p ~/.local/bin
cp /tmp/cass-prime-release-target/release/cass ~/.local/bin/cass-prime
chmod +x ~/.local/bin/cass-prime

# Prove the three binaries are distinct files and versions
ls -l /opt/homebrew/bin/cass ~/.local/bin/cass-letta ~/.local/bin/cass-prime
cass --version
~/.local/bin/cass-letta --version
~/.local/bin/cass-prime --version
```

Expected:

| Command | Version |
|---|---|
| `cass --version` | `cass 0.6.24` |
| `cass-letta --version` | `cass 0.6.24-letta.1` |
| `cass-prime --version` | `cass 0.6.25-letta-prime.1` |

If `/tmp/cass-prime-release-target/release/cass` is gone, rebuild from the
tagged commit without installing into Homebrew:

```bash
cd /Users/kyle/Code/coding_agent_session_search
git checkout v0.6.25-letta-prime.1
CARGO_TARGET_DIR=/tmp/cass-prime-release-target cargo build --release --bin cass
cp /tmp/cass-prime-release-target/release/cass ~/.local/bin/cass-prime
```

## Wrapper

Define this in the terminal you will use for the Prime fork (or add it to
`~/.zshrc`). Keep `cass_letta_safe` unchanged if you still have it.

```bash
PRIME_SANDBOX="$HOME/.local/share/cass-prime-sandbox"
mkdir -p "$PRIME_SANDBOX"

cass_prime_safe() {
  env -u CASS_DAEMON_SOCKET \
    CASS_DATA_DIR="$PRIME_SANDBOX" \
    CASS_SKIP_UPDATE=1 \
    CODING_AGENT_SEARCH_NO_UPDATE_PROMPT=1 \
    /Users/kyle/.local/bin/cass-prime "$@"
}
```

Keep using Homebrew `cass` in other terminals as usual. Keep using
`cass_letta_safe` if you still want the older Letta-only sandbox.

## Verify isolation, then index

1. Snapshot the live DB **and** the Letta sandbox so you can prove neither
   moved:

   ```bash
   LIVE="$HOME/Library/Application Support/com.coding-agent-search.coding-agent-search"
   LETTA_SANDBOX="$HOME/.local/share/cass-letta-sandbox"
   stat -f 'LIVE db size=%z mtime=%Sm' "$LIVE/agent_search.db"
   stat -f 'LETTA sandbox db size=%z mtime=%Sm' "$LETTA_SANDBOX/agent_search.db"
   df -h "$HOME/.local/share"
   ```

2. Confirm the wrapper is bound to the **Prime** sandbox:

   ```bash
   cass_prime_safe robot-docs paths
   cass_prime_safe health --json
   cass_prime_safe capabilities --json | jq '{version, crate_version, has_prime: (.connectors | index("prime_agent") != null), has_letta: (.connectors | index("letta_code") != null), has_pi: (.connectors | index("pi_agent") != null), count: (.connectors | length)}'
   ```

   `data dir default` / `data_dir` must be `.../cass-prime-sandbox`. A fresh
   sandbox reports `healthy: false`. That is expected (no DB yet).
   Capabilities must show version `0.6.25-letta-prime.1`, `prime_agent`,
   `letta_code`, `pi_agent`, and 26 connectors.

3. Re-check the live and Letta snapshots (`stat` + Homebrew `cass health --json`).
   Same size and mtime as step 1.

4. Lexical ingest into the Prime sandbox only. First run on an empty sandbox
   is a full scan of every local connector, including Prime and Letta. Do not
   pass `--watch` or `--semantic` for this check.

   ```bash
   cass_prime_safe index --json
   ```

   Success looks like `"success": true`, `"data_dir"` and `"db_path"` under
   `cass-prime-sandbox`, `"prime_agent"` and `"letta_code"` in
   `indexing_stats.connectors` / `agents_discovered`, and
   `"quarantined_conversations": 0`.

5. Search the sandbox. This machine has real Prime sessions under
   `~/.prime/agent/sessions` (45 `.jsonl` files at the time of writing).

   ```bash
   cass_prime_safe search "*" --agent prime_agent --robot --limit 5 --fields summary
   cass_prime_safe search "*" --agent pi_agent --robot --limit 5 --fields summary
   cass_prime_safe search "*" --agent letta_code --robot --limit 5 --fields summary
   cass_prime_safe search "authentication" --robot --limit 5 --fields summary
   ```

   Prime hits must report `"agent": "prime_agent"`, never `"pi_agent"`.
   Pi hits must stay `"pi_agent"`. Letta should still appear. The last query
   should hit your usual agents (Claude, Codex, …).

6. Optional local-only resume check (does **not** need to launch Prime if
   the executable is missing; the argv/error is the proof):

   ```bash
   # Pick one real local session path from the search hits, then:
   cass_prime_safe resume /absolute/path/to/session.jsonl --json
   ```

   Expect `prime-agent --resume <that absolute path>` as one argv item.
   A missing `prime-agent` on PATH is a clear error, not a launch of `pi`.
   Do not point resume at a remote-only / mirrored path.

7. Prove the live archive and Letta sandbox are still the snapshots from
   step 1:

   ```bash
   stat -f 'LIVE db size=%z mtime=%Sm' "$LIVE/agent_search.db"
   stat -f 'LETTA sandbox db size=%z mtime=%Sm' "$LETTA_SANDBOX/agent_search.db"
   cass health --json
   ls "$HOME/.local/share/cass-prime-sandbox/agent_search.db"
   ```

## Cleanup

Deleting `~/.local/share/cass-prime-sandbox` removes only the Prime-check
copy. Do not delete:

- `~/Library/Application Support/com.coding-agent-search.coding-agent-search`
- `~/.local/share/cass-letta-sandbox`
- `~/.prime/agent/sessions` (those are the real Prime logs)

Removing `~/.local/bin/cass-prime` removes the extra binary (and, after
cutover, breaks daily `cass` and the LaunchAgents). Do not do that on the
cutover machine. `cass-letta` and the two sandbox data dirs stay.

## When you are ready to stop sandboxing

That switch already happened on this machine (2026-08-16). The ordered
steps, LaunchAgent edits, incremental live index, Homebrew uninstall, and
rollback notes are in [CASS_PRIME_CUTOVER.md](CASS_PRIME_CUTOVER.md).

If you are on a *different* machine that still uses Homebrew `cass` as the
daily driver, do not point `cass-prime` at that live data dir until you
mean to. Until then, every Prime-fork command goes through `cass_prime_safe`.
