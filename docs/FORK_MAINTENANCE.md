# Fork maintenance: Letta Code + Prime Agent + FAD pin

This branch is a private fork of CASS. Do **not** open a pull request against
`Dicklesworthstone/coding_agent_session_search`.

Current fork identity:

- CASS origin: `klittle32/coding_agent_session_search`
- CASS version: `0.6.25-letta-prime.1` (prerelease derived from upstream `0.6.24`)
- FAD origin: `klittle32/franken_agent_detection`
- FAD pin: `34d543ab5417ba04dc657ee08aa82fad8bc2eca4` (`0.1.12-letta-prime.1`, tag `v0.1.12-letta-prime.1`)
- Sibling checkout expected at `../franken_agent_detection` on that same SHA
- Self-update target: `klittle32/coding_agent_session_search` (`src/update_check.rs`)

Letta Code and Prime Agent parsing live only in FAD. CASS exposes re-export
stubs (`src/connectors/letta_code.rs`, `src/connectors/prime_agent.rs`) and
must not grow a second parser.

Prime Agent identity:

- Canonical slug: `prime_agent` (never collapse into `pi_agent`)
- Display label: `Prime Agent`
- Default: `~/.prime/agent/sessions/<session-id>.jsonl`
- Overrides: `PRIME_AGENT_SESSION_DIR`, `PRIME_AGENT_CODING_AGENT_SESSION_DIR`,
  `PRIME_AGENT_CODING_AGENT_DIR` (agent dir; sessions appended)
- Resume: `prime-agent --resume <local path>`
- Projection: complete append-only history, including branches
- Custom per-run `--session-dir` paths require a persistent environment
  override or explicit CASS source configuration

## FAD update cycle

1. Fetch `upstream/main` in the FAD fork.
2. Review changes to normalized types, the `Connector` trait, scan/discovery
   helpers, factory/probe registries, and the newest JSONL connector.
3. Rebase or merge the private Letta + Prime connector branch.
4. Resolve registry/count conflicts deliberately.
5. Re-run all FAD checks and Letta/Prime fixtures.
6. Bump the fork prerelease identifier (do not reuse an upstream release version).
7. Freeze, tag, and record the immutable full SHA.

## CASS update cycle

1. Fetch `upstream/main` in this CASS fork.
2. Rebase or merge, preserving:
   - fork package/repository identity
   - fork update target
   - Letta and Prime module/stubs
   - exhaustive provider surfaces
3. Update the FAD dependency as one atomic unit:
   - `Cargo.toml`
   - `Cargo.lock`
   - `build.rs` `DependencyContract` (`expected_git`, `expected_rev`,
     `expected_version`, `patch_url`; keep `repo_rel = "../franken_agent_detection"`)
4. Re-run the exhaustive `rg` sweeps from the Prime/Letta integration plans
   (FAD URL/SHA/version claims and connector-count/list/golden surfaces).
5. Re-run targeted and full tests, plus
   `CASS_STRICT_PATH_DEP_VALIDATION=1 cargo check --all-features`.
6. Run the fabricated sentinel end-to-end fixtures (`LETTA_TRANSCRIPT_ROOT` /
   default `~/.prime/agent/sessions` + `CASS_SKIP_UPDATE=1`).
7. Tag a new CASS fork build (example: `v0.6.25-letta-prime.1`).

## Remote custom-root caveat

FAD honors `LETTA_TRANSCRIPT_ROOT` and `PRIME_AGENT_SESSION_DIR` locally.
CASS’s generic remote probe API emits tilde-relative default paths
(`~/.letta/transcripts`, `~/.prime/agent/sessions`) and cannot necessarily
discover an arbitrary environment override on a remote noninteractive shell.
Default remote roots are required. A custom remote root may need explicit
`sources.toml` configuration; do not broaden the probe API just to chase a
remote env override.

## Side-by-side use with Homebrew cass

The fork binaries (`cass-letta`, `cass-prime`) and Homebrew `cass` share the
same default data directory. Until you trust the fork, run it only against a
throwaway data dir. See [CASS_LETTA_SANDBOX.md](CASS_LETTA_SANDBOX.md) and
[CASS_PRIME_SANDBOX.md](CASS_PRIME_SANDBOX.md) for the wrappers, isolation
rules, and worked ingests that index Letta/Prime plus existing agents without
writing the live archive.

## Drift alarm

Any CASS update that changes the FAD pin, connector factory count, normalized
message schema, source discovery contract, resume dispatch, update checker, or
golden capability schema requires a full Letta + Prime integration pass — not
merely a compile check.
