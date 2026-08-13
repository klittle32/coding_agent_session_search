# Fork maintenance: Letta Code + FAD pin

This branch is a private fork of CASS. Do **not** open a pull request against
`Dicklesworthstone/coding_agent_session_search`.

Current fork identity:

- CASS origin: `klittle32/coding_agent_session_search`
- CASS version: `0.6.24-letta.1` (prerelease derived from upstream `0.6.24`)
- FAD origin: `klittle32/franken_agent_detection`
- FAD pin: `394ba2a22773c1f63f701145383d28867797974e` (`0.1.11-letta.1`, tag `fad-letta-v0.1.11-letta.1`)
- Sibling checkout expected at `../franken_agent_detection` on that same SHA
- Self-update target: `klittle32/coding_agent_session_search` (`src/update_check.rs`)

Letta Code parsing lives only in FAD. CASS exposes a re-export stub
(`src/connectors/letta_code.rs`) and must not grow a second parser.

## FAD update cycle

1. Fetch `upstream/main` in the FAD fork.
2. Review changes to normalized types, the `Connector` trait, scan/discovery
   helpers, factory/probe registries, and the newest JSONL connector.
3. Rebase or merge the private Letta connector branch.
4. Resolve registry/count conflicts deliberately.
5. Re-run all FAD checks and Letta fixtures.
6. Bump the fork prerelease identifier (do not reuse an upstream release version).
7. Freeze, tag, and record the immutable full SHA.

## CASS update cycle

1. Fetch `upstream/main` in this CASS fork.
2. Rebase or merge, preserving:
   - fork package/repository identity
   - fork update target
   - Letta module/stub
   - exhaustive provider surfaces
3. Update the FAD dependency as one atomic unit:
   - `Cargo.toml`
   - `Cargo.lock`
   - `build.rs` `DependencyContract` (`expected_git`, `expected_rev`,
     `expected_version`, `patch_url`; keep `repo_rel = "../franken_agent_detection"`)
4. Re-run the exhaustive `rg` sweeps from the Letta integration plan
   (FAD URL/SHA/version claims and connector-count/list/golden surfaces).
5. Re-run targeted and full tests, plus
   `cargo check --features strict-path-dep-validation`.
6. Run the fabricated sentinel end-to-end fixture (`LETTA_TRANSCRIPT_ROOT` +
   `CASS_SKIP_UPDATE=1`).
7. Tag a new CASS fork build (example: `cass-letta-v0.6.24-letta.1`).

## Remote custom-root caveat

FAD honors `LETTA_TRANSCRIPT_ROOT` locally. CASS’s generic remote probe API
emits tilde-relative default paths (`~/.letta/transcripts`) and cannot
necessarily discover an arbitrary environment override on a remote
noninteractive shell. Default remote `~/.letta/transcripts` support is
required. A custom remote root may need explicit `sources.toml` configuration;
do not broaden the probe API just to chase a remote env override.

## Drift alarm

Any CASS update that changes the FAD pin, connector factory count, normalized
message schema, source discovery contract, update checker, or golden capability
schema requires a full Letta integration pass — not merely a compile check.
