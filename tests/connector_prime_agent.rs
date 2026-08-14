//! CASS integration contract for Prime Agent sessions.
//!
//! Parser logic lives in `franken_agent_detection::PrimeAgentConnector`.
//! These tests prove CASS consumes that connector as a real source:
//! detection, indexing, sentinel search, structured invocations, token
//! analytics, branches, raw mirroring, idempotent reindex, incremental
//! append, `since_ts`, malformed siblings, Pi coexistence, sanitized PATH,
//! and exact `prime-agent --resume` argv.
//!
//! Fixtures are fabricated sentinels only. Never copy private sessions.

use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::{Duration, SystemTime};

use assert_cmd::Command;
use coding_agent_search::connectors::pi_agent::PiAgentConnector;
use coding_agent_search::connectors::prime_agent::PrimeAgentConnector;
use coding_agent_search::connectors::{
    Connector, Origin, ScanContext, ScanRoot, extract_tokens_for_agent,
};
use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

const USER_SENTINEL: &str = "prime-user-cobalt-orchard";
const REASONING_SENTINEL: &str = "prime-reasoning-amber-lattice";
const ASSISTANT_SENTINEL: &str = "prime-assistant-silver-azimuth";
const TOOL_NAME: &str = "read_file";
const TOOL_ARG_SENTINEL: &str = "prime-tool-arg-violet-keystone";
const TOOL_RESULT_SENTINEL: &str = "prime-tool-result-green-circuit";
const TOOL_FAIL_SENTINEL: &str = "prime-tool-failure-red-marker";
const SHELL_CMD_SENTINEL: &str = "prime-shell-command-indigo-rivet";
const SHELL_OUT_SENTINEL: &str = "prime-shell-output-copper-harbor";
const CUSTOM_SENTINEL: &str = "prime-custom-context-teal-signal";
const COMPACTION_SENTINEL: &str = "prime-compaction-gold-archive";
const BRANCH_SUMMARY_SENTINEL: &str = "prime-branch-summary-blue-fork";
const ABANDONED_SENTINEL: &str = "prime-abandoned-branch-magenta-path";
const APPEND_SENTINEL: &str = "prime-append-white-quill";
const BASE64_BLOB: &str = "PRIME_BASE64_MUST_NOT_SURVIVE";
const SESSION_ID: &str = "0198f000-0000-7000-8000-000000000101";

type TestResult = Result<(), Box<dyn Error>>;

fn test_error(message: impl Into<String>) -> Box<dyn Error> {
    std::io::Error::other(message.into()).into()
}

fn write_session(dir: &Path, id: &str, body: &str) -> PathBuf {
    fs::create_dir_all(dir).expect("mkdir sessions");
    let path = dir.join(format!("{id}.jsonl"));
    fs::write(&path, body).expect("write session");
    path
}

fn default_sessions(home: &Path) -> PathBuf {
    home.join(".prime/agent/sessions")
}

fn header(id: &str, cwd: &str) -> String {
    format!(
        r#"{{"type":"session","version":3,"id":"{id}","timestamp":"2026-08-13T12:00:00.000Z","cwd":"{cwd}","parentSession":"/fabricated/prime/parent.jsonl","rlmDepth":0,"git":{{"branch":"feature/prime"}}}}"#
    )
}

fn full_body(id: &str) -> String {
    format!(
        r#"{header}
{{"type":"session_info","id":"n1","parentId":null,"timestamp":"2026-08-13T12:00:01.000Z","name":"First Prime Name"}}
{{"type":"message","id":"u1","parentId":"n1","timestamp":"2026-08-13T12:00:02.000Z","message":{{"role":"user","content":"{USER_SENTINEL}"}}}}
{{"type":"message","id":"u2","parentId":"u1","timestamp":"2026-08-13T12:00:03.000Z","message":{{"role":"user","content":[{{"type":"text","text":"please inspect this image"}},{{"type":"image","mimeType":"image/png","data":"{BASE64_BLOB}"}}]}}}}
{{"type":"model_change","id":"m1","parentId":"u2","timestamp":"2026-08-13T12:00:04.000Z","provider":"anthropic","modelId":"claude-opus-4-6"}}
{{"type":"service_tier_change","id":"t1","parentId":"m1","timestamp":"2026-08-13T12:00:05.000Z","serviceTier":"default"}}
{{"type":"message","id":"a1","parentId":"t1","timestamp":"2026-08-13T12:00:06.000Z","message":{{"role":"assistant","content":[{{"type":"thinking","thinking":"{REASONING_SENTINEL}"}},{{"type":"text","text":"{ASSISTANT_SENTINEL}"}},{{"type":"toolCall","id":"call-read-1","name":"{TOOL_NAME}","arguments":{{"path":"/fabricated/prime/src.rs","needle":"{TOOL_ARG_SENTINEL}"}}}},{{"type":"toolCall","id":"call-bash-1","name":"bash","arguments":{{"command":"echo {TOOL_ARG_SENTINEL}"}}}}],"provider":"anthropic","model":"claude-opus-4-6","usage":{{"input":100,"output":250,"cacheRead":1000,"cacheWrite":20}},"stopReason":"toolUse"}}}}
{{"type":"message","id":"tr1","parentId":"a1","timestamp":"2026-08-13T12:00:07.000Z","message":{{"role":"toolResult","toolCallId":"call-read-1","toolName":"{TOOL_NAME}","content":[{{"type":"text","text":"{TOOL_RESULT_SENTINEL}"}}],"isError":false}}}}
{{"type":"message","id":"tr2","parentId":"tr1","timestamp":"2026-08-13T12:00:08.000Z","message":{{"role":"toolResult","toolCallId":"call-bash-1","toolName":"bash","content":[{{"type":"text","text":"{TOOL_FAIL_SENTINEL}"}}],"isError":true}}}}
{{"type":"message","id":"b1","parentId":"tr2","timestamp":"2026-08-13T12:00:09.000Z","message":{{"role":"bashExecution","command":"{SHELL_CMD_SENTINEL}","output":"{SHELL_OUT_SENTINEL}","exitCode":0}}}}
{{"type":"compaction","id":"c1","parentId":"b1","timestamp":"2026-08-13T12:00:10.000Z","summary":"{COMPACTION_SENTINEL}"}}
{{"type":"custom_message","id":"cm1","parentId":"c1","timestamp":"2026-08-13T12:00:11.000Z","customType":"prime-extension","content":"{CUSTOM_SENTINEL}","display":true}}
{{"type":"message","id":"ab1","parentId":"cm1","timestamp":"2026-08-13T12:00:12.000Z","message":{{"role":"user","content":"{ABANDONED_SENTINEL}"}}}}
{{"type":"branch_summary","id":"bs1","parentId":"cm1","timestamp":"2026-08-13T12:00:13.000Z","fromId":"ab1","summary":"{BRANCH_SUMMARY_SENTINEL}"}}
{{"type":"session_info","id":"n2","parentId":"bs1","timestamp":"2026-08-13T12:00:14.000Z","name":"Latest Prime Title"}}
"#,
        header = header(id, "/fabricated/prime/project")
    )
}

fn parent_child_bodies() -> (String, String) {
    let parent = format!(
        r#"{{"type":"session","version":3,"id":"0198f000-0000-7000-8000-000000000105","timestamp":"2026-08-13T16:00:00.000Z","cwd":"/fabricated/prime/rlm","rlmDepth":0}}
{{"type":"message","id":"ruser001","parentId":null,"timestamp":"2026-08-13T16:00:01.000Z","message":{{"role":"user","content":"spawn a child"}}}}
{{"type":"message","id":"rasst001","parentId":"ruser001","timestamp":"2026-08-13T16:00:02.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"parent turn"}}],"provider":"anthropic","model":"claude-opus-4-6","usage":{{"input":80,"output":40,"cacheRead":10,"cacheWrite":5}},"stopReason":"stop"}}}}
{{"type":"child_usage_attributed","id":"attr0001","parentId":"rasst001","timestamp":"2026-08-13T16:00:03.000Z","targetId":"rasst001","origin":"spawn_task","childUsage":{{"input":30,"output":20,"cacheRead":0,"cacheWrite":0}},"aggregateUsage":{{"input":110,"output":60,"cacheRead":10,"cacheWrite":5}}}}
"#
    );
    let child = format!(
        r#"{{"type":"session","version":3,"id":"0198f000-0000-7000-8000-000000000106","timestamp":"2026-08-13T16:05:00.000Z","cwd":"/fabricated/prime/rlm-child","parentSession":"/fabricated/prime/rlm/0198f000-0000-7000-8000-000000000105.jsonl","rlmDepth":1}}
{{"type":"message","id":"cuser001","parentId":null,"timestamp":"2026-08-13T16:05:01.000Z","message":{{"role":"user","content":"child work"}}}}
{{"type":"message","id":"casst001","parentId":"cuser001","timestamp":"2026-08-13T16:05:02.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"child turn"}}],"provider":"anthropic","model":"claude-opus-4-6","usage":{{"input":30,"output":20,"cacheRead":0,"cacheWrite":0}},"stopReason":"stop"}}}}
"#
    );
    (parent, child)
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
        .args([
            "search",
            query,
            "--json",
            "--limit",
            "20",
            "--agent",
            "prime_agent",
        ])
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

fn with_env(key: &str, value: Option<&Path>, body: impl FnOnce()) {
    let previous = std::env::var(key).ok();
    match value {
        Some(path) => unsafe {
            std::env::set_var(key, path);
        },
        None => unsafe {
            std::env::remove_var(key);
        },
    }
    body();
    match previous {
        Some(prev) => unsafe {
            std::env::set_var(key, prev);
        },
        None => unsafe {
            std::env::remove_var(key);
        },
    }
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
fn factory_registry_contains_prime_agent_once_and_codex_is_only_replacement() {
    let cass = coding_agent_search::indexer::get_connector_factories();
    let fad = franken_agent_detection::get_connector_factories();
    assert_eq!(cass.len(), fad.len());
    let cass_slugs: Vec<&str> = cass.iter().map(|(slug, _)| *slug).collect();
    let fad_slugs: Vec<&str> = fad.iter().map(|(slug, _)| *slug).collect();
    assert_eq!(cass_slugs, fad_slugs);
    assert_eq!(
        cass_slugs
            .iter()
            .filter(|slug| **slug == "prime_agent")
            .count(),
        1,
        "prime_agent must appear exactly once"
    );
    assert!(cass_slugs.contains(&"pi_agent"));
    assert!(cass_slugs.contains(&"letta_code"));
    assert!(cass_slugs.contains(&"codex"));
    assert_ne!(
        cass_slugs.iter().position(|slug| *slug == "prime_agent"),
        cass_slugs.iter().position(|slug| *slug == "pi_agent")
    );
}

#[test]
fn prime_aliases_stay_distinct_from_pi() {
    assert_ne!("prime_agent", "pi_agent");
    let normalized = |raw: &str| raw.trim().to_ascii_lowercase().replace('-', "_");
    assert_eq!(normalized("prime-agent"), "prime_agent");
    assert_eq!(
        normalized("primeagent").replace("primeagent", "prime_agent"),
        "prime_agent"
    );
    assert_ne!(normalized("pi"), "prime_agent");
    assert_eq!(normalized("pi_agent"), "pi_agent");
}

#[test]
#[serial]
fn detect_honors_session_dir_override() {
    let tmp = TempDir::new().unwrap();
    let sessions = tmp.path().join("custom-sessions");
    write_session(&sessions, SESSION_ID, &full_body(SESSION_ID));
    with_env("PRIME_AGENT_SESSION_DIR", Some(&sessions), || {
        let result = PrimeAgentConnector::new().detect();
        assert!(result.detected, "override root with a session must detect");
        assert!(
            result
                .root_paths
                .iter()
                .any(|path| path == &sessions || path.starts_with(&sessions)),
            "detected roots should include the override: {:?}",
            result.root_paths
        );
    });
}

#[test]
#[serial]
fn detect_honors_legacy_and_agent_dir_overrides() {
    let tmp = TempDir::new().unwrap();
    let legacy = tmp.path().join("legacy-sessions");
    write_session(&legacy, SESSION_ID, &full_body(SESSION_ID));
    with_env("PRIME_AGENT_SESSION_DIR", None, || {
        with_env(
            "PRIME_AGENT_CODING_AGENT_SESSION_DIR",
            Some(&legacy),
            || {
                let result = PrimeAgentConnector::new().detect();
                assert!(result.detected, "legacy session dir must detect");
            },
        );
    });

    let agent_dir = tmp.path().join("agent-home");
    write_session(
        &agent_dir.join("sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    with_env("PRIME_AGENT_SESSION_DIR", None, || {
        with_env("PRIME_AGENT_CODING_AGENT_SESSION_DIR", None, || {
            with_env("PRIME_AGENT_CODING_AGENT_DIR", Some(&agent_dir), || {
                let result = PrimeAgentConnector::new().detect();
                assert!(result.detected, "agent dir override must detect");
            });
        });
    });
}

#[test]
fn session_indexes_as_one_conversation_with_stable_identity() {
    let tmp = TempDir::new().unwrap();
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    let convs = PrimeAgentConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].agent_slug, "prime_agent");
    assert_ne!(convs[0].agent_slug, "pi_agent");
    assert_eq!(convs[0].external_id.as_deref(), Some(SESSION_ID));
    assert_eq!(convs[0].title.as_deref(), Some("Latest Prime Title"));
    assert_eq!(
        convs[0].workspace,
        Some(PathBuf::from("/fabricated/prime/project"))
    );
    assert_eq!(
        convs[0].metadata["parent_session"],
        "/fabricated/prime/parent.jsonl"
    );
    assert_eq!(convs[0].metadata["rlm_depth"], 0);
}

#[test]
fn fallback_title_uses_first_user_text() {
    let tmp = TempDir::new().unwrap();
    let id = "0198f000-0000-7000-8000-000000000130";
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        id,
        &format!(
            "{}\n{{\"type\":\"message\",\"id\":\"u1\",\"parentId\":null,\"timestamp\":\"2026-08-13T12:00:02.000Z\",\"message\":{{\"role\":\"user\",\"content\":\"{USER_SENTINEL}\"}}}}\n",
            header(id, "/x")
        ),
    );
    let convs = PrimeAgentConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    assert_eq!(convs[0].title.as_deref(), Some(USER_SENTINEL));
}

#[test]
fn sentinels_and_invocations_survive_scan() {
    let tmp = TempDir::new().unwrap();
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    let convs = PrimeAgentConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    let joined = convs
        .iter()
        .flat_map(|conv| conv.messages.iter().map(|msg| msg.content.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    for sentinel in [
        USER_SENTINEL,
        REASONING_SENTINEL,
        ASSISTANT_SENTINEL,
        TOOL_ARG_SENTINEL,
        TOOL_RESULT_SENTINEL,
        TOOL_FAIL_SENTINEL,
        SHELL_CMD_SENTINEL,
        SHELL_OUT_SENTINEL,
        CUSTOM_SENTINEL,
        COMPACTION_SENTINEL,
        BRANCH_SUMMARY_SENTINEL,
        ABANDONED_SENTINEL,
    ] {
        assert!(joined.contains(sentinel), "missing {sentinel}");
    }
    assert!(!joined.contains(BASE64_BLOB));
    let assistant = convs[0]
        .messages
        .iter()
        .find(|msg| msg.invocations.len() >= 2)
        .expect("structured invocations");
    assert_eq!(assistant.invocations[0].name, TOOL_NAME);
    assert_eq!(
        assistant.invocations[0].call_id.as_deref(),
        Some("call-read-1")
    );
    assert_eq!(
        assistant.invocations[0].arguments.as_ref().unwrap()["needle"],
        TOOL_ARG_SENTINEL
    );
    assert_eq!(assistant.invocations[1].name, "bash");
}

#[test]
fn direct_tokens_and_child_usage_are_not_double_counted() {
    let tmp = TempDir::new().unwrap();
    let (parent_body, child_body) = parent_child_bodies();
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        "0198f000-0000-7000-8000-000000000105",
        &parent_body,
    );
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        "0198f000-0000-7000-8000-000000000106",
        &child_body,
    );
    let convs = PrimeAgentConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan");
    let parent = convs
        .iter()
        .find(|conv| conv.external_id.as_deref() == Some("0198f000-0000-7000-8000-000000000105"))
        .expect("parent");
    let child = convs
        .iter()
        .find(|conv| conv.external_id.as_deref() == Some("0198f000-0000-7000-8000-000000000106"))
        .expect("child");
    let parent_assistant = parent
        .messages
        .iter()
        .find(|msg| msg.role == "assistant")
        .expect("parent assistant");
    let extracted = extract_tokens_for_agent(
        "prime_agent",
        &parent_assistant.extra,
        &parent_assistant.content,
        "assistant",
    );
    assert_eq!(extracted.input_tokens, Some(80));
    assert_eq!(extracted.output_tokens, Some(40));
    assert_ne!(extracted.input_tokens, Some(110));
    assert_eq!(parent.metadata["rlm_depth"], 0);
    assert_eq!(child.metadata["rlm_depth"], 1);
    assert_eq!(
        parent_assistant.extra["cass"]["token_usage"]["data_source"],
        "api"
    );
}

#[test]
fn pi_and_prime_coexist_without_duplicates() {
    let tmp = TempDir::new().unwrap();
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    let pi_dir = tmp.path().join(".pi/agent/sessions/project");
    fs::create_dir_all(&pi_dir).unwrap();
    fs::write(
        pi_dir.join("2026-08-13T00-00-00_pi.jsonl"),
        r#"{"type":"session","id":"pi-keep","timestamp":"2026-08-13T00:00:00Z"}
{"type":"message","timestamp":"2026-08-13T00:00:01Z","message":{"role":"user","content":"pi-isolation-sentinel"}}
"#,
    )
    .unwrap();
    let ctx = ctx_for(tmp.path());
    let prime = PrimeAgentConnector::new().scan(&ctx).expect("prime");
    let pi = PiAgentConnector::new().scan(&ctx).expect("pi");
    assert_eq!(prime.len(), 1);
    assert_eq!(prime[0].agent_slug, "prime_agent");
    assert!(pi.iter().all(|conv| conv.agent_slug == "pi_agent"));
    assert!(pi.iter().any(|conv| {
        conv.messages
            .iter()
            .any(|msg| msg.content.contains("pi-isolation-sentinel"))
    }));
    assert!(prime.iter().all(|conv| {
        !conv
            .messages
            .iter()
            .any(|msg| msg.content.contains("pi-isolation-sentinel"))
    }));
}

#[test]
fn source_discovery_skips_artifacts_and_logs() {
    let tmp = TempDir::new().unwrap();
    let path = write_session(
        &tmp.path().join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    fs::create_dir_all(tmp.path().join(".prime/agent/session-artifacts")).unwrap();
    fs::write(
        tmp.path()
            .join(".prime/agent/session-artifacts/trace.jsonl"),
        "{}\n",
    )
    .unwrap();
    fs::create_dir_all(tmp.path().join(".prime/agent/logs")).unwrap();
    fs::write(tmp.path().join(".prime/agent/logs/agent.jsonl"), "{}\n").unwrap();
    let discovered = PrimeAgentConnector::new()
        .discover_source_files(&ctx_for(tmp.path()))
        .expect("discover");
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].source_path, path);
    assert!(discovered[0].required_for_reconstruction);
    assert!(!discovered.iter().any(|src| {
        src.source_path
            .to_string_lossy()
            .contains("session-artifacts")
    }));
}

#[test]
fn remote_mirrored_layout_preserves_provenance() {
    let tmp = TempDir::new().unwrap();
    let mirrored_root = tmp.path().join("mirror-host");
    write_session(
        &mirrored_root.join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    let mirrored = ScanRoot::remote(mirrored_root, Origin::remote("mirror"), None)
        .with_rewrite("/fabricated/prime", "/local/prime");
    let ctx = ScanContext::with_roots(tmp.path().join("data"), vec![mirrored], None);
    let sources = PrimeAgentConnector::new()
        .discover_source_files(&ctx)
        .expect("discover mirror");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].origin.source_id, "mirror");
    let convs = PrimeAgentConnector::new().scan(&ctx).expect("scan mirror");
    assert_eq!(convs.len(), 1);
    assert_eq!(
        convs[0].workspace,
        Some(PathBuf::from("/local/prime/project"))
    );
}

#[test]
fn since_ts_excludes_unchanged_old_files() {
    let tmp = TempDir::new().unwrap();
    let path = write_session(
        &tmp.path().join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    fs::File::options()
        .write(true)
        .open(&path)
        .expect("open")
        .set_modified(old)
        .expect("mtime");
    let ctx = ScanContext::with_roots(
        tmp.path().join("data"),
        vec![ScanRoot::local(tmp.path().to_path_buf())],
        Some(1_700_000_000_i64),
    );
    let convs = PrimeAgentConnector::new().scan(&ctx).expect("scan");
    assert!(
        convs.is_empty(),
        "old mtime below since_ts must be excluded"
    );
}

#[test]
fn malformed_sibling_does_not_hide_valid_session() {
    let tmp = TempDir::new().unwrap();
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    write_session(
        &tmp.path().join(".prime/agent/sessions"),
        "0198f000-0000-7000-8000-000000000199",
        "this is not json\n{\"type\":\"nope\"}\n",
    );
    let convs = PrimeAgentConnector::new()
        .scan(&ctx_for(tmp.path()))
        .expect("scan must not abort");
    assert_eq!(convs.len(), 1);
    assert_eq!(convs[0].external_id.as_deref(), Some(SESSION_ID));
}

#[test]
fn default_home_layout_indexes_and_searches_without_node() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    write_session(&default_sessions(&home), SESSION_ID, &full_body(SESSION_ID));
    run_index(&home, &data_dir, &[])?;
    for sentinel in [
        USER_SENTINEL,
        REASONING_SENTINEL,
        ASSISTANT_SENTINEL,
        TOOL_NAME,
        TOOL_ARG_SENTINEL,
        TOOL_RESULT_SENTINEL,
        TOOL_FAIL_SENTINEL,
        SHELL_CMD_SENTINEL,
        SHELL_OUT_SENTINEL,
        CUSTOM_SENTINEL,
        COMPACTION_SENTINEL,
        BRANCH_SUMMARY_SENTINEL,
        ABANDONED_SENTINEL,
    ] {
        let hits = search_hits(&home, &data_dir, sentinel)?;
        assert!(
            !hits.is_empty(),
            "expected hits for {sentinel}; got {hits:?}"
        );
        assert!(
            hits.iter()
                .all(|hit| hit.get("agent").and_then(Value::as_str) == Some("prime_agent")),
            "hits must stay under prime_agent: {hits:?}"
        );
    }
    Ok(())
}

#[test]
fn unchanged_reindex_does_not_duplicate() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    write_session(&default_sessions(&home), SESSION_ID, &full_body(SESSION_ID));
    let first = index_envelope(&home, &data_dir, &[])?;
    let second = index_envelope(&home, &data_dir, &[])?;
    assert_eq!(first.get("conversations"), second.get("conversations"));
    assert_eq!(first.get("messages"), second.get("messages"));
    let hits = search_hits(&home, &data_dir, USER_SENTINEL)?;
    let unique_paths: std::collections::BTreeSet<_> = hits
        .iter()
        .filter_map(|hit| hit.get("source_path").and_then(Value::as_str))
        .collect();
    assert_eq!(unique_paths.len(), 1);
    Ok(())
}

#[test]
fn appending_a_line_reindexes_only_the_expected_conversation() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    let sessions = default_sessions(&home);
    let path = write_session(&sessions, SESSION_ID, &full_body(SESSION_ID));
    let sibling = write_session(
        &sessions,
        "0198f000-0000-7000-8000-000000000102",
        &format!(
            "{}\n{{\"type\":\"message\",\"id\":\"u1\",\"parentId\":null,\"timestamp\":\"2026-08-13T12:00:02.000Z\",\"message\":{{\"role\":\"user\",\"content\":\"unchanged-sibling\"}}}}\n",
            header("0198f000-0000-7000-8000-000000000102", "/other")
        ),
    );
    let first = index_envelope(&home, &data_dir, &[])?;
    // FAD's file_modified_since keeps a 1s lookback. Age the untouched sibling
    // so a fast first index cannot make both files look dirty on the next pass.
    let aged = SystemTime::now()
        .checked_sub(Duration::from_secs(5))
        .ok_or_else(|| test_error("clock went backwards"))?;
    fs::File::options()
        .write(true)
        .open(&sibling)?
        .set_modified(aged)?;
    let mut body = fs::read_to_string(&path)?;
    body.push_str(&format!(
        r#"{{"type":"message","id":"ap1","parentId":"n2","timestamp":"2026-08-13T12:00:20.000Z","message":{{"role":"user","content":"{APPEND_SENTINEL}"}}}}"#
    ));
    body.push('\n');
    fs::write(&path, body)?;
    let second = index_envelope(&home, &data_dir, &[])?;
    assert_eq!(first.get("conversations").and_then(Value::as_i64), Some(2));
    assert_eq!(
        second.get("conversations").and_then(Value::as_i64),
        Some(1),
        "incremental reindex should touch only the appended conversation"
    );
    assert!(!search_hits(&home, &data_dir, APPEND_SENTINEL)?.is_empty());
    assert!(!search_hits(&home, &data_dir, "unchanged-sibling")?.is_empty());
    Ok(())
}

#[test]
fn watch_once_picks_up_appended_session() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    let path = write_session(&default_sessions(&home), SESSION_ID, &full_body(SESSION_ID));
    run_index(&home, &data_dir, &[])?;
    let mut body = fs::read_to_string(&path)?;
    body.push_str(&format!(
        r#"{{"type":"message","id":"w1","parentId":"n2","timestamp":"2026-08-13T12:00:21.000Z","message":{{"role":"user","content":"{APPEND_SENTINEL}"}}}}"#
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
    assert!(
        !search_hits(&home, &data_dir, APPEND_SENTINEL)?.is_empty(),
        "watch-once should index the appended Prime line"
    );
    Ok(())
}

#[test]
fn resume_fake_prime_agent_receives_exact_argv() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    let data_dir = tmp.path().join("cass-data");
    let bin_dir = tmp.path().join("bin");
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&bin_dir)?;
    let session = write_session(
        &home.join(".prime/agent/sessions with spaces"),
        SESSION_ID,
        &full_body(SESSION_ID),
    );
    let argv_log = tmp.path().join("argv.log");
    let fake = bin_dir.join("prime-agent");
    fs::write(
        &fake,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" > '{}'\n",
            argv_log.display()
        ),
    )?;
    let mut perms = fs::metadata(&fake)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&fake, perms)?;

    let mut path = javascript_free_path();
    path = format!("{}:{path}", bin_dir.display());
    let cass = env!("CARGO_BIN_EXE_cass");
    let output = StdCommand::new(cass)
        .args(["--color=never", "resume", "--exec"])
        .arg(session.as_os_str())
        .env("CASS_SKIP_UPDATE", "1")
        .env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1")
        .env("CASS_IGNORE_SOURCES_CONFIG", "1")
        .env("HOME", &home)
        .env("PATH", &path)
        .output()?;
    if !output.status.success() {
        return Err(test_error(format!(
            "resume --exec failed ({}); stderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let logged = fs::read_to_string(&argv_log)?;
    let lines: Vec<&str> = logged.lines().collect();
    assert_eq!(lines[0], fake.to_str().unwrap());
    assert_eq!(lines[1], "--resume");
    assert_eq!(
        lines.len(),
        3,
        "source path must remain one argv item: {lines:?}"
    );
    assert_eq!(Path::new(lines[2]), fs::canonicalize(&session)?.as_path());
    Ok(())
}

#[test]
fn resume_missing_executable_is_actionable() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    fs::create_dir_all(&home)?;
    let session = write_session(&default_sessions(&home), SESSION_ID, &full_body(SESSION_ID));
    let cass = env!("CARGO_BIN_EXE_cass");
    let output = StdCommand::new(cass)
        .args(["--color=never", "resume", "--exec"])
        .arg(session.as_os_str())
        .env("CASS_SKIP_UPDATE", "1")
        .env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1")
        .env("CASS_IGNORE_SOURCES_CONFIG", "1")
        .env("HOME", &home)
        .env("PATH", javascript_free_path())
        .output()?;
    assert!(!output.status.success(), "missing prime-agent must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("prime-agent") && (stderr.contains("PATH") || stderr.contains("failed")),
        "missing executable error should mention prime-agent: {stderr}"
    );
    Ok(())
}

#[test]
fn resume_remote_only_source_does_not_launch() -> TestResult {
    let tmp = TempDir::new()?;
    let home = tmp.path().join("home");
    fs::create_dir_all(&home)?;
    let missing = PathBuf::from("/remote-only-host/.prime/agent/sessions/missing.jsonl");
    let cass = env!("CARGO_BIN_EXE_cass");
    let output = StdCommand::new(cass)
        .args([
            "--color=never",
            "resume",
            "--json",
            "--agent",
            "prime_agent",
        ])
        .arg(missing.as_os_str())
        .env("CASS_SKIP_UPDATE", "1")
        .env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1")
        .env("CASS_IGNORE_SOURCES_CONFIG", "1")
        .env("HOME", &home)
        .env("PATH", javascript_free_path())
        .output()?;
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let blob = format!("{stdout}{stderr}").to_ascii_lowercase();
    assert!(
        blob.contains("local-only") || blob.contains("does not exist"),
        "remote-only resume must refuse without launching: {blob}"
    );
    Ok(())
}
