//! CASS integration contract for Letta Code client transcripts.
//!
//! Parser logic lives in `franken_agent_detection::LettaCodeConnector`.
//! These tests prove CASS consumes that connector as a real source:
//! detection, indexing, sentinel search, structured invocations,
//! idempotent reindex, incremental append, `since_ts`, source mirroring,
//! malformed siblings, no backend/reflection ingest, `LETTA_TRANSCRIPT_ROOT`,
//! default mirrored layout, and watch-once when the harness allows it.
//!
//! Fixtures are fabricated sentinels only. Never copy private transcripts.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, SystemTime};

use assert_cmd::Command;
use coding_agent_search::connectors::letta_code::LettaCodeConnector;
use coding_agent_search::connectors::{Connector, Origin, ScanContext, ScanRoot};
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

const USER_SENTINEL: &str = "letta-user-cobalt-lantern";
const REASONING_SENTINEL: &str = "letta-reasoning-amber-compass";
const ASSISTANT_SENTINEL: &str = "letta-assistant-silver-orbit";
const TOOL_ARG_SENTINEL: &str = "letta-tool-arg-violet-key";
const TOOL_RESULT_SENTINEL: &str = "letta-tool-result-green-beacon";
const TOOL_FAIL_SENTINEL: &str = "letta-tool-failure-red-signal";
const APPEND_SENTINEL: &str = "letta-append-indigo-quill";

type TestResult = Result<(), Box<dyn Error>>;

fn test_error(message: impl Into<String>) -> Box<dyn Error> {
    std::io::Error::other(message.into()).into()
}

fn write_transcript(root: &Path, agent: &str, conversation: &str, body: &str) -> PathBuf {
    let dir = root.join(agent).join(conversation);
    fs::create_dir_all(&dir).expect("mkdir conversation");
    let path = dir.join("transcript.jsonl");
    fs::write(&path, body).expect("write transcript");
    path
}

fn valid_text_body() -> String {
    format!(
        "{{\"kind\":\"user\",\"text\":\"{USER_SENTINEL}\",\"captured_at\":\"2026-07-01T12:00:00Z\",\"source_message_id\":\"msg-shared-orbit\"}}\n\
         {{\"kind\":\"reasoning\",\"text\":\"{REASONING_SENTINEL}\",\"captured_at\":\"2026-07-01T12:00:01Z\",\"source_message_id\":\"msg-shared-orbit\"}}\n\
         {{\"kind\":\"assistant\",\"text\":\"{ASSISTANT_SENTINEL}\",\"captured_at\":\"2026-07-01T12:00:02Z\",\"source_message_id\":\"msg-shared-orbit\"}}\n"
    )
}

fn tools_body() -> String {
    [
        r#"{"kind":"user","text":"please inspect the fixture file","captured_at":"2026-07-01T13:00:00Z","source_message_id":"msg-user-tools"}"#,
        &format!(
            r#"{{"kind":"tool_call","name":"Read","argsText":"{{\"file_path\":\"src/lib.rs\",\"needle\":\"{TOOL_ARG_SENTINEL}\"}}","resultText":"{TOOL_RESULT_SENTINEL}","resultOk":true,"captured_at":"2026-07-01T13:00:01Z","source_line_id":"line-read-1","source_message_id":"msg-read-1"}}"#
        ),
        &format!(
            r#"{{"kind":"tool_call","name":"Shell","argsText":"{{\"cmd\":\"fail\"}}","resultText":"{TOOL_FAIL_SENTINEL}","resultOk":false,"captured_at":"2026-07-01T13:00:02Z","source_line_id":"line-fail-1"}}"#
        ),
    ]
    .join("\n")
        + "\n"
}

fn combined_body() -> String {
    format!("{}{}", valid_text_body(), tools_body())
}

fn ctx_for(root: &Path) -> ScanContext {
    ScanContext::with_roots(
        root.join("cass-data"),
        vec![ScanRoot::local(root.to_path_buf())],
        None,
    )
}

fn javascript_free_path() -> String {
    const SKIP: &[&str] = &["node", "nodejs", "npm", "npx", "bun"];
    std::env::var("PATH")
        .unwrap_or_else(|_| "/usr/bin:/bin".to_string())
        .split(':')
        .filter(|dir| {
            !dir.is_empty()
                && Path::new(dir).is_dir()
                && SKIP.iter().all(|bin| !Path::new(dir).join(bin).is_file())
        })
        .collect::<Vec<_>>()
        .join(":")
}

fn isolated_cass(home: &Path, data_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("cass").expect("cass binary");
    cmd.env("CASS_SKIP_UPDATE", "1")
        .env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1")
        .env("CASS_IGNORE_SOURCES_CONFIG", "1")
        .env("RUST_MIN_STACK", "16777216")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("PATH", javascript_free_path())
        .args(["--color=never"])
        .args(["--data-dir", data_dir.to_str().expect("utf8 data dir")]);
    cmd
}

fn run_index(home: &Path, data_dir: &Path, extra_env: &[(&str, &str)]) -> TestResult {
    let mut cmd = isolated_cass(home, data_dir);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let output = cmd
        .args(["index", "--json", "--no-progress-events"])
        .output()?;
    if !output.status.success() {
        return Err(test_error(format!(
            "cass index failed ({}); stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn index_envelope(
    home: &Path,
    data_dir: &Path,
    extra_env: &[(&str, &str)],
) -> Result<Value, Box<dyn Error>> {
    let mut cmd = isolated_cass(home, data_dir);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let output = cmd
        .args(["index", "--json", "--no-progress-events"])
        .output()?;
    if !output.status.success() {
        return Err(test_error(format!(
            "cass index failed ({}); stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn search_hits(home: &Path, data_dir: &Path, query: &str) -> Result<Vec<Value>, Box<dyn Error>> {
    let output = isolated_cass(home, data_dir)
        .args(["search", query, "--json", "--limit", "20"])
        .output()?;
    if !output.status.success() {
        return Err(test_error(format!(
            "cass search {query:?} failed ({}); stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let parsed: Value = serde_json::from_slice(&output.stdout)?;
    Ok(parsed
        .get("hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

#[test]
fn javascript_free_path_omits_node_npm_npx_bun() {
    let path = javascript_free_path();
    for bin in ["node", "nodejs", "npm", "npx", "bun"] {
        for dir in path.split(':').filter(|dir| !dir.is_empty()) {
            assert!(
                !Path::new(dir).join(bin).is_file(),
                "{bin} must not be present on the sanitized PATH entry {dir}"
            );
        }
    }
}

#[test]
fn factory_registry_contains_letta_code_once_and_codex_is_only_replacement() {
    let cass = coding_agent_search::indexer::get_connector_factories();
    let fad = franken_agent_detection::get_connector_factories();
    assert_eq!(cass.len(), fad.len());
    let cass_slugs: Vec<&str> = cass.iter().map(|(slug, _)| *slug).collect();
    let fad_slugs: Vec<&str> = fad.iter().map(|(slug, _)| *slug).collect();
    assert_eq!(cass_slugs, fad_slugs);
    assert_eq!(
        cass_slugs
            .iter()
            .filter(|slug| **slug == "letta_code")
            .count(),
        1,
        "letta_code must appear exactly once"
    );
    assert!(cass_slugs.contains(&"codex"));
}

#[test]
#[serial]
fn detect_honors_temporary_letta_root() {
    let tmp = TempDir::new().unwrap();
    let transcripts = tmp.path().join("transcripts");
    write_transcript(
        &transcripts,
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    let previous = std::env::var("LETTA_TRANSCRIPT_ROOT").ok();
    unsafe {
        std::env::set_var("LETTA_TRANSCRIPT_ROOT", &transcripts);
    }
    let result = LettaCodeConnector::new().detect();
    match previous {
        Some(value) => unsafe {
            std::env::set_var("LETTA_TRANSCRIPT_ROOT", value);
        },
        None => unsafe {
            std::env::remove_var("LETTA_TRANSCRIPT_ROOT");
        },
    }
    assert!(
        result.detected,
        "override root with a transcript must detect"
    );
    assert!(
        result
            .root_paths
            .iter()
            .any(|path| path == &transcripts || path.starts_with(&transcripts)),
        "detected roots should include the override: {:?}",
        result.root_paths
    );
}

#[test]
fn transcript_indexes_as_one_conversation_with_stable_external_id() {
    let tmp = TempDir::new().unwrap();
    write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    let convs = LettaCodeConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].agent_slug, "letta_code");
    assert_eq!(
        convs[0].external_id.as_deref(),
        Some("agent-alpha/conversation-text")
    );
    assert!(convs[0].workspace.is_none());
}

#[test]
fn sentinels_are_searchable_and_reasoning_is_authored() {
    let tmp = TempDir::new().unwrap();
    write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-tools",
        &tools_body(),
    );
    let convs = LettaCodeConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    let contents: Vec<String> = convs
        .iter()
        .flat_map(|conv| conv.messages.iter().map(|msg| msg.content.clone()))
        .collect();
    let joined = contents.join("\n");
    assert!(joined.contains(USER_SENTINEL));
    assert!(joined.contains(REASONING_SENTINEL));
    assert!(joined.contains(ASSISTANT_SENTINEL));
    assert!(joined.contains(TOOL_ARG_SENTINEL));
    assert!(joined.contains(TOOL_RESULT_SENTINEL));
    assert!(joined.contains(TOOL_FAIL_SENTINEL));

    let reasoning = convs
        .iter()
        .flat_map(|conv| conv.messages.iter())
        .find(|msg| msg.content.contains(REASONING_SENTINEL))
        .expect("reasoning message");
    assert_eq!(reasoning.role, "assistant");
    assert_eq!(reasoning.author.as_deref(), Some("reasoning"));
}

#[test]
fn invocations_are_structured() {
    let tmp = TempDir::new().unwrap();
    write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-tools",
        &tools_body(),
    );
    let convs = LettaCodeConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    let invocation = convs
        .iter()
        .flat_map(|conv| conv.messages.iter())
        .find_map(|msg| msg.invocations.first())
        .expect("structured invocation");
    assert_eq!(invocation.kind, "tool");
    assert_eq!(invocation.name, "Read");
    assert_eq!(invocation.call_id.as_deref(), Some("line-read-1"));
    let args = invocation.arguments.as_ref().expect("arguments");
    assert_eq!(args["needle"], TOOL_ARG_SENTINEL);
}

#[test]
fn since_ts_excludes_unchanged_old_files() {
    let tmp = TempDir::new().unwrap();
    let path = write_transcript(tmp.path(), "agent-a", "conv-a", &valid_text_body());
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    fs::File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(old)
        .expect("mtime");
    let cutoff = 1_700_000_000_i64;
    let ctx = ScanContext::with_roots(
        tmp.path().join("data"),
        vec![ScanRoot::local(tmp.path().to_path_buf())],
        Some(cutoff),
    );
    let convs = LettaCodeConnector::new().scan(&ctx).expect("scan");
    assert!(
        convs.is_empty(),
        "old mtime below since_ts must be excluded"
    );
}

#[test]
fn source_discovery_includes_exact_transcript() {
    let tmp = TempDir::new().unwrap();
    let path = write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    let discovered = LettaCodeConnector::new()
        .discover_source_files(&ctx_for(tmp.path()))
        .expect("discover");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].source_path, path);
    assert!(discovered[0].required_for_reconstruction);
}

#[test]
fn malformed_sibling_does_not_hide_valid_transcript() {
    let tmp = TempDir::new().unwrap();
    write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    write_transcript(
        tmp.path(),
        "agent-gamma",
        "conversation-invalid",
        "this is not json\n{\"kind\":\"nope\"}\n",
    );
    let convs = LettaCodeConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan must not abort");
    assert_eq!(convs.len(), 1);
    assert_eq!(
        convs[0].external_id.as_deref(),
        Some("agent-alpha/conversation-text")
    );
}

#[test]
fn backend_and_reflection_payloads_are_not_ingested() {
    let tmp = TempDir::new().unwrap();
    write_transcript(
        tmp.path(),
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    fs::write(
        tmp.path().join("lc-local-backend.json"),
        r#"{"kind":"backend"}"#,
    )
    .unwrap();
    fs::write(
        tmp.path().join("memory-reflection.json"),
        r#"{"kind":"reflection","text":"secret-reflection"}"#,
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join("agent-alpha/conversation-text")).unwrap();
    fs::write(
        tmp.path()
            .join("agent-alpha/conversation-text/payload-manifest.json"),
        r#"{"kind":"manifest"}"#,
    )
    .unwrap();
    let discovered = LettaCodeConnector::new()
        .discover_source_files(&ctx_for(tmp.path()))
        .expect("discover");
    assert_eq!(discovered.len(), 1);
    assert!(discovered[0].source_path.ends_with("transcript.jsonl"));
    let convs = LettaCodeConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    let joined = convs
        .iter()
        .flat_map(|conv| conv.messages.iter().map(|msg| msg.content.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!joined.contains("secret-reflection"));
}

#[test]
fn custom_local_letta_transcript_root_indexes_and_searches() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    let transcripts = tmp.path().join("custom-root");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    write_transcript(
        &transcripts,
        "agent-alpha",
        "conversation-text",
        &combined_body(),
    );
    run_index(
        &home,
        &data_dir,
        &[("LETTA_TRANSCRIPT_ROOT", transcripts.to_str().unwrap())],
    )?;
    for sentinel in [
        USER_SENTINEL,
        REASONING_SENTINEL,
        ASSISTANT_SENTINEL,
        TOOL_ARG_SENTINEL,
        TOOL_RESULT_SENTINEL,
        TOOL_FAIL_SENTINEL,
    ] {
        let hits = search_hits(&home, &data_dir, sentinel)?;
        assert!(
            !hits.is_empty(),
            "expected hits for {sentinel}; got {hits:?}"
        );
    }
    Ok(())
}

#[test]
fn default_home_layout_indexes_without_override() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    let transcripts = home.join(".letta").join("transcripts");
    write_transcript(
        &transcripts,
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    run_index(&home, &data_dir, &[])?;
    let hits = search_hits(&home, &data_dir, USER_SENTINEL)?;
    assert!(!hits.is_empty(), "default ~/.letta/transcripts must index");
    Ok(())
}

#[test]
fn default_remote_mirrored_layout_preserves_provenance() {
    let tmp = TempDir::new().unwrap();
    let mirrored_root = tmp.path().join("mirror-host");
    write_transcript(
        &mirrored_root.join(".letta").join("transcripts"),
        "agent-a",
        "conv-a",
        &valid_text_body(),
    );
    let mirrored = ScanRoot::remote(mirrored_root.clone(), Origin::remote("mirror"), None);
    let ctx = ScanContext::with_roots(tmp.path().join("data"), vec![mirrored], None);
    let sources = LettaCodeConnector::new()
        .discover_source_files(&ctx)
        .expect("discover mirror");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].origin.source_id, "mirror");
    let convs = LettaCodeConnector::new().scan(&ctx).expect("scan mirror");
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].external_id.as_deref(), Some("agent-a/conv-a"));
}

#[test]
fn unchanged_reindex_does_not_duplicate() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    let transcripts = home.join(".letta").join("transcripts");
    write_transcript(
        &transcripts,
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    let first = index_envelope(&home, &data_dir, &[])?;
    let second = index_envelope(&home, &data_dir, &[])?;
    assert_eq!(first.get("conversations"), second.get("conversations"));
    assert_eq!(first.get("messages"), second.get("messages"));
    let hits = search_hits(&home, &data_dir, USER_SENTINEL)?;
    let unique_paths: std::collections::BTreeSet<_> = hits
        .iter()
        .filter_map(|hit| hit.get("source_path").and_then(Value::as_str))
        .collect();
    assert_eq!(
        unique_paths.len(),
        1,
        "unchanged reindex must not duplicate hits"
    );
    Ok(())
}

#[test]
fn appending_a_line_reindexes_only_the_expected_conversation() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    let transcripts = home.join(".letta").join("transcripts");
    let path = write_transcript(
        &transcripts,
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    write_transcript(
        &transcripts,
        "agent-beta",
        "conversation-other",
        "{\"kind\":\"user\",\"text\":\"unchanged-sibling\",\"captured_at\":\"2026-07-01T12:00:00Z\"}\n",
    );
    let first = index_envelope(&home, &data_dir, &[])?;
    let mut body = fs::read_to_string(&path)?;
    body.push_str(&format!(
        r#"{{"kind":"assistant","text":"{APPEND_SENTINEL}","captured_at":"2026-07-01T12:00:09Z"}}"#
    ));
    body.push('\n');
    fs::write(&path, body)?;
    let second = index_envelope(&home, &data_dir, &[])?;
    assert_eq!(
        first.get("conversations").and_then(Value::as_i64),
        Some(2),
        "initial index should ingest both Letta conversations"
    );
    assert_eq!(
        second.get("conversations").and_then(Value::as_i64),
        Some(1),
        "incremental reindex should touch only the appended conversation"
    );
    let hits = search_hits(&home, &data_dir, APPEND_SENTINEL)?;
    assert!(!hits.is_empty(), "appended sentinel must be searchable");
    let sibling = search_hits(&home, &data_dir, "unchanged-sibling")?;
    assert!(
        !sibling.is_empty(),
        "untouched sibling must remain searchable"
    );
    Ok(())
}

#[test]
fn watch_once_picks_up_appended_transcript() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    let transcripts = home.join(".letta").join("transcripts");
    let path = write_transcript(
        &transcripts,
        "agent-alpha",
        "conversation-text",
        &valid_text_body(),
    );
    run_index(&home, &data_dir, &[])?;
    let mut body = fs::read_to_string(&path)?;
    body.push_str(&format!(
        r#"{{"kind":"user","text":"{APPEND_SENTINEL}","captured_at":"2026-07-01T12:00:10Z"}}"#
    ));
    body.push('\n');
    fs::write(&path, body)?;

    let cass = env!("CARGO_BIN_EXE_cass");
    let output = StdCommand::new(cass)
        .args(["--color=never", "index", "--watch", "--watch-once"])
        .arg(path.to_str().unwrap())
        .args(["--data-dir", data_dir.to_str().unwrap()])
        .env("CASS_SKIP_UPDATE", "1")
        .env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1")
        .env("CASS_IGNORE_SOURCES_CONFIG", "1")
        .env("RUST_MIN_STACK", "16777216")
        .env("HOME", &home)
        .env("PATH", javascript_free_path())
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .output()?;
    if !output.status.success() {
        return Err(test_error(format!(
            "watch-once exited {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let hits = search_hits(&home, &data_dir, APPEND_SENTINEL)?;
    assert!(
        !hits.is_empty(),
        "watch-once should index the appended Letta line"
    );
    Ok(())
}
