# Live cutover: `cass-prime` became the daily driver

This is the record of the 2026-08-16 cutover on Kyle’s Mac. The sandbox
runbook ([CASS_PRIME_SANDBOX.md](CASS_PRIME_SANDBOX.md)) is how we *proved*
the fork. This document is how we *switched* the live archive and LaunchAgents
onto that binary, then removed Homebrew `cass` so it could not be invoked by
accident.

**After reading this you should be able to** say what `cass` is on this
machine, how watch/daemon are wired, what the first live lexical index and
the later MiniLM semantic pass did, and how to roll back without repeating
the mistakes we avoided.

## Current state (after cutover)

| Role | Path | Version / notes |
|---|---|---|
| Daily `cass` | `~/.local/bin/cass` | 193-byte shim; `exec`s `cass-prime` |
| Real binary | `~/.local/bin/cass-prime` | `0.6.25-letta-prime.1` |
| Sandbox wrapper | `~/.local/bin/cass-prime-safe` | Forces `CASS_DATA_DIR=~/.local/share/cass-prime-sandbox`, unsets `CASS_DAEMON_SOCKET` |
| Older Letta fork | `~/.local/bin/cass-letta` | `0.6.24-letta.1`; still sandbox-only |
| Homebrew `cass` | *(removed)* | Was Cellar `0.6.24`; `brew uninstall --formula cass` on 2026-08-16. Not `--zap`. |
| Live archive | `~/Library/Application Support/com.coding-agent-search.coding-agent-search` | Unchanged location. SQLite is source of truth. |
| Prime sandbox | `~/.local/share/cass-prime-sandbox` | Left in place; not the daily store |
| Letta sandbox | `~/.local/share/cass-letta-sandbox` | Left in place; not the daily store |
| Quality vectors | live `vector_index` / MiniLM-384 | Caught up 2026-08-16 (~1h 36m semantic pass). 4,254 conversations / 411,925 docs. |

`~/.local/bin` is ahead of `/opt/homebrew/bin` on `PATH`. After uninstall,
`type cass` resolves only to the shim.

There is no `~/.config/cass` and no `sources.toml`. This machine is
local-only. Shell env still has `CASS_DAEMON_SOCKET` (live daemon socket) and
`CASS_SEMANTIC_EMBEDDER=minilm`.

Self-update on this binary targets `klittle32/coding_agent_session_search`,
not Dicklesworthstone and not Homebrew. Do not run `brew install cass` later:
that reinstalls upstream without Prime or Letta.

## Decisions

These were explicit, not defaults.

1. **Do not `brew upgrade cass`.** Homebrew already advertised upstream
   `0.6.24 → 0.6.25`. That bottle is Dicklesworthstone’s build. It does not
   include `prime_agent` or `letta_code`.
2. **Do not replace the Cellar binary in place.** Keep the fork as its own
   file (`cass-prime`) so LaunchAgents and the sandbox wrapper have a stable
   path.
3. **Stop the live writers before pointing the fork at the live archive.**
   Both LaunchAgents use `KeepAlive`. Killing the PIDs is not enough;
   `launchctl bootout` is required. At cutover start, Homebrew watch (pid
   4782, since Thursday) and daemon (pid 750, since 30 Jul) were running, and
   the live WAL was ~15 GB.
4. **First live write is incremental lexical only:** `cass index --json`.
   No `--full` (would rebuild the ~50 GB archive). No `--watch` (launchd owns
   watch). No `--semantic` (hybrid catch-up is a separate one-shot; watch is
   lexical-only on purpose — cass#258 / #269 / #311).
5. **Plists call `cass-prime` directly**, not the shim. The shim is for
   interactive `PATH`. Launchd should not depend on a wrapper that could be
   confused with `cass-prime-safe`.
6. **Leave the `cass-prime` filename.** A second 48 MB copy would drift. A
   rename without a leftover `cass-prime` name would break LaunchAgents and
   `cass-prime-safe`. The shim is enough now that Homebrew is gone.
7. **Uninstall Homebrew `cass` after the fork was proven on live.** Pin first
   (so a mid-cutover `brew upgrade` could not sneak upstream in), then
   `brew uninstall --formula cass`. Never `--zap` — that is a cask hammer and
   we would not risk the Application Support archive even if it applied.
8. **Keep both sandboxes and `cass-prime-safe`.** They are the isolation
   hatch if we need to test a new fork build without touching live.
9. **Do not run** `doctor --fix`, `upgrade`, `models install`, or bare TUI
   `cass` as part of cutover. MiniLM was already on disk.

## What we did (order)

Times are local, 2026-08-16.

1. **Preflight.** Homebrew `cass 0.6.24` was healthy on the live data dir,
   watch active, semantic `building` (pre-existing). Capabilities: Homebrew
   24 connectors, no Prime/Letta. Fork: 26 connectors, both present. 45 Prime
   `.jsonl` files and 73 Letta transcripts on disk. `cass` was not brew-pinned.
2. **`brew pin cass`.** Prevented an accidental upstream bottle install during
   the window where Homebrew was still present.
3. **Back up LaunchAgents** (copy, not delete):

   ```text
   ~/Library/LaunchAgents/com.kyle.cass.daemon.plist.homebrew-cass.bak.20260816T063030
   ~/Library/LaunchAgents/com.kyle.cass.index-watch.plist.homebrew-cass.bak.20260816T063030
   ```

   Those backups still name `/opt/homebrew/bin/cass`. They are useless until
   a Homebrew `cass` exists again.
4. **Unload writers.** Watch first, then daemon:

   ```bash
   launchctl bootout "gui/$(id -u)/com.kyle.cass.index-watch"
   launchctl bootout "gui/$(id -u)/com.kyle.cass.daemon"
   ```

   Both PIDs exited. A stale `index-run.lock` from dead pid 4782 was left on
   disk (we do not delete lock files). The incremental index reclaimed it.
5. **Install the daily shim** at `~/.local/bin/cass` (mode 755):

   ```sh
   #!/bin/sh
   exec /Users/kyle/.local/bin/cass-prime "$@"
   ```

6. **Retarget both plists** `ProgramArguments[0]` from
   `/opt/homebrew/bin/cass` to `/Users/kyle/.local/bin/cass-prime`. Env,
   socket, lexical-only watch flags, and log paths were left as they were.
7. **Read-only check** with the shim (no `CASS_DATA_DIR` override):
   `cass health --json` → live data dir, healthy, `watch_active=false`.
   `cass capabilities --json` → `0.6.25-letta-prime.1`, 26 connectors,
   `prime_agent` + `letta_code`.
8. **Incremental live index** (launchd still unloaded):

   ```bash
   cass --color=never index --json
   ```

   Exit 0 in ~435 s. Entrypoint `kind=incremental`, `semantic=false`.
   `data_dir` / `db_path` were the live Application Support paths.
   `quarantined_conversations=0`. Connector stats for this pass:

   | Agent | Indexed now | Messages in this pass |
   |---|---|---|
   | `prime_agent` | 32 | 5,995 |
   | `letta_code` | 67 | 26,662 |
   | `claude` | 318 | 2,788 |
   | `cursor` | 14 | 1,338 (2 `state.vscdb` locked by the open app) |
   | `codex` | 10 | 719 |
   | `grok` | 5 | 716 |

   Lexical strategy: `incremental_inline`. Cursor lock-busy is retryable;
   watch picks those up later. It is not a zero-message import.
9. **Reload LaunchAgents** (plist path, so launchd reads the new program):

   ```bash
   launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.kyle.cass.daemon.plist
   launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.kyle.cass.index-watch.plist
   ```

   Both services came up as `/Users/kyle/.local/bin/cass-prime`. Immediately
   after reload, `cass health` reported `rebuilding` / `watch_startup`. That
   is expected. A later health check was `healthy`.
10. **Prove search on live.** `cass search … --agent prime_agent` and
    `--agent letta_code` returned hits with those slugs (not `pi_agent`).
    Sandbox DB mtimes were still 2026-08-14 / 2026-08-13.
11. **Remove Homebrew `cass`** after the fork was the daily driver:

    ```bash
    brew unpin cass
    brew uninstall --formula cass
    ```

    `/opt/homebrew/bin/cass` and `Cellar/cass` are gone. Live DB inode and
    mtime did not change across uninstall. The `dicklesworthstone/tap` tap
    can stay; the formula must stay uninstalled.

## LaunchAgents (as left)

Both agents: `RunAtLoad=true`, `KeepAlive=true`, `Nice=5`,
`CASS_SEMANTIC_EMBEDDER=minilm`,
`CASS_DAEMON_SOCKET=/Users/kyle/Library/Caches/cass/daemon.sock`,
`TMPDIR=/Users/kyle/Library/Caches/cass/tmp`,
`PATH=/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin` (for *other* tools;
the program itself is an absolute `cass-prime` path).

### `com.kyle.cass.index-watch`

```text
/Users/kyle/.local/bin/cass-prime index --watch --watch-interval 30
```

Lexical + file watch only. Extra env:
`CASS_SKIP_PREFLIGHT_COUNT_TOTAL_MESSAGES=1`. Logs:
`~/Library/Logs/cass/index-watch.{out,err}.log`.

Do **not** add `--semantic` to this agent. Semantic/hybrid vectors are a
one-shot (`cass index --semantic --json`) or `cass models backfill`.

### `com.kyle.cass.daemon`

```text
/Users/kyle/.local/bin/cass-prime daemon --idle-timeout 0 --socket /Users/kyle/Library/Caches/cass/daemon.sock
```

Keeps MiniLM warm for hybrid embed/search. Logs:
`~/Library/Logs/cass/daemon.{out,err}.log`.

## Hybrid / semantic pass (same day, after cutover)

Default *search* is hybrid-preferred and fail-opens to lexical. The
incremental `cass index --json` does **not** embed. After cutover, hybrid
was already working on the **old** corpus (`fully_hybrid_refined`, no
fallback). The quality MiniLM index was stale relative to the live DB:

- Built 2026-08-13: 3,810 conversations / 380,402 docs, `minilm-384`
- `current_db_matches: false` after the Prime/Letta lexical ingest
- Fast/hash progressive tier absent (`pending_work` stays true for that
  unused tier). Leave it alone. `hnsw_ready` is false; only needed for
  `--approximate`.
- Watch does not embed. `pending.sessions` was 0, so another lexical
  `cass index --json` was not required. Ignore stale-at-~80s / `--full`
  nudges while watch is healthy.

MiniLM was already installed and verified
(`models/all-MiniLM-L6-v2`, revision `c9745ed1…`). No `models install`.

### What we ran (2026-08-16 06:51–08:27 local)

1. **Unload watch only.** It held `index-run.lock` (pid 89765). Daemon
   stayed up so MiniLM stayed warm:

   ```bash
   launchctl bootout "gui/$(id -u)/com.kyle.cass.index-watch"
   ```

   Stale lock from the dead watch pid was left on disk. The semantic job
   reclaimed it (`job_kind=semantic_rebuild`, pid 8926).
2. **One-shot semantic backfill** (no `--full`, no `--watch`, no
   `--build-hnsw`):

   ```bash
   cass --color=never index --semantic --json
   ```

   Exit 0 in 5,787,389 ms (~1h 36m). Entrypoint `kind=semantic_backfill`.
   Live `data_dir` / `db_path`. `quarantined_conversations=0`. Phases
   included `semantic_initialize`, `semantic_replay`,
   `semantic_embedding` (411,930 docs), `semantic_finalize`. Peak embed
   rate ~90–140 docs/s, later ~30 docs/s. Along the way it also indexed 2
   Cursor sessions that were lock-busy during the morning lexical pass.
3. **Reload watch:**

   ```bash
   launchctl bootstrap "gui/$(id -u)" ~/Library/LaunchAgents/com.kyle.cass.index-watch.plist
   ```

   Health may say `rebuilding` for a short window (`watch_startup`). That
   is watch, not a failed semantic job.

### Result

| | Before pass | After pass |
|---|---|---|
| Quality conversations | 3,810 (2026-08-13) | 4,254 |
| Quality docs | 380,402 | 411,925 |
| `quality_tier_remaining` | 0 (but DB fingerprint stale) | 0 |
| Hybrid probe | `fully_hybrid_refined` on old corpus | `fully_hybrid_refined`, no fallback |

Do **not** add `--semantic` to the watch LaunchAgent. Future catch-up is
the same one-shot if health/status show `current_db_matches: false` again.

## Rollback

Homebrew `cass` is no longer installed, so “restore plists and unpin” is
incomplete.

1. Unload both agents (`bootout` the two labels).
2. `brew install dicklesworthstone/tap/cass` — this is **upstream**, no
   Prime/Letta.
3. Restore the `.homebrew-cass.bak.20260816T063030` plists (they name
   `/opt/homebrew/bin/cass`).
4. Rename or remove `~/.local/bin/cass` so `PATH` does not shadow Homebrew.
5. `launchctl bootstrap` both plists.

Prefer staying on the fork. Reinstalling Homebrew is how you accidentally
lose Prime/Letta in the daily binary again.

## Isolation hatch (unchanged)

```bash
cass-prime-safe health --json    # sandbox only
cass health --json               # live archive
```

Do not run `cass-prime` without the safe wrapper if you intend to stay in
the sandbox. Bare `cass-prime` uses the same default data dir as daily
`cass`.

## Related

- [CASS_PRIME_SANDBOX.md](CASS_PRIME_SANDBOX.md) — pre-cutover isolation proof
- [CASS_LETTA_SANDBOX.md](CASS_LETTA_SANDBOX.md) — older Letta-only sandbox
- [FORK_MAINTENANCE.md](FORK_MAINTENANCE.md) — FAD pin and no-upstream-PR policy
