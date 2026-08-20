//! Command-level tests for connector-backed raw `cass export`.
//!
//! These invoke the real binary with the issue-reproduction syntax:
//! `cass export --format markdown -- <session-path>`.
//! Fixtures are synthetic sentinels only.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use coding_agent_search::connectors::grok::GrokConnector;
use coding_agent_search::connectors::letta_code::LettaCodeConnector;
use coding_agent_search::connectors::{Connector, ScanContext, ScanRoot};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

const GROK_USER: &str = "GROK_USER_SENTINEL_alpha";
const GROK_ASSISTANT: &str = "GROK_ASSISTANT_SENTINEL_beta";
const GROK_TOOL: &str = "GROK_TOOL_SENTINEL_delta";
const GROK_BLOB: &str = "GROK_BLOB_SENTINEL_base64_QVlBQUFBQUE=";
const GROK_DUPLICATE: &str = "GROK_DUPLICATE_FALLBACK_MUST_NOT_APPEAR";
const GROK_FALLBACK_USER: &str = "GROK_FALLBACK_USER";
const GROK_FALLBACK_ASSISTANT: &str = "GROK_FALLBACK_ASSISTANT";
const GENERIC_USER: &str = "GENERIC_USER_SENTINEL";
const GENERIC_ASSISTANT: &str = "GENERIC_ASSISTANT_SENTINEL";
const CODEX_USER: &str = "CODEX_NESTED_USER";
const CODEX_ASSISTANT: &str = "CODEX_NESTED_ASSISTANT";
const LETTA_USER: &str = "LETTA_USER_SENTINEL_cobalt";
const LETTA_ASSISTANT: &str = "LETTA_ASSISTANT_SENTINEL_silver";
const LETTA_REASONING: &str = "LETTA_REASONING_SENTINEL_amber";
const LETTA_TOOL_RESULT: &str = "LETTA_TOOL_RESULT_SENTINEL";
const LETTA_BLOB: &str = "LETTA_BLOB_SENTINEL_base64_QVlBQUFBQUE=";

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/raw_export")
        .join(rel)
}

fn isolated_cass(home: &Path, data_dir: &Path) -> Command {
    let mut cmd = Command::cargo_bin("cass").expect("cass binary");
    let db_path = data_dir.join("cass.db");
    cmd.env("CASS_SKIP_UPDATE", "1")
        .env("CODING_AGENT_SEARCH_NO_UPDATE_PROMPT", "1")
        .env("CASS_IGNORE_SOURCES_CONFIG", "1")
        .env("RUST_MIN_STACK", "16777216")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .args(["--color", "never"])
        .args(["--db", db_path.to_str().expect("utf8 db path")]);
    cmd
}

fn export_markdown(path: &Path) -> Output {
    let tmp = TempDir::new().expect("tempdir");
    let data_dir = tmp.path().join("cass-data");
    isolated_cass(tmp.path(), &data_dir)
        .args([
            "export",
            "--format",
            "markdown",
            "--",
            path.to_str().expect("utf8 path"),
        ])
        .output()
        .expect("run cass export")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(output),
        stderr(output)
    );
}

fn heading_unknown_count(markdown: &str) -> usize {
    markdown
        .lines()
        .filter(|line| line.trim().eq_ignore_ascii_case("## unknown"))
        .count()
}

fn has_user_heading(markdown: &str) -> bool {
    markdown.contains("## 👤 User")
        || markdown
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case("## user"))
}

fn has_assistant_heading(markdown: &str) -> bool {
    markdown.contains("## 🤖 Assistant")
        || markdown
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case("## assistant"))
}

fn ordered_user_assistant(markdown: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current_role: Option<String> = None;
    let mut body = String::new();
    let flush = |role: &Option<String>, body: &str, out: &mut Vec<(String, String)>| {
        if let Some(role) = role
            && matches!(role.as_str(), "user" | "assistant")
        {
            let text = body.trim();
            if !text.is_empty() {
                out.push((role.clone(), text.to_string()));
            }
        }
    };
    for line in markdown.lines() {
        let trimmed = line.trim();
        let role = if trimmed == "## 👤 User" || trimmed.eq_ignore_ascii_case("## user") {
            Some("user")
        } else if trimmed == "## 🤖 Assistant" || trimmed.eq_ignore_ascii_case("## assistant") {
            Some("assistant")
        } else {
            None
        };
        if let Some(role) = role {
            flush(&current_role, &body, &mut out);
            current_role = Some(role.to_string());
            body.clear();
            continue;
        }
        if trimmed == "---" || trimmed.starts_with('#') {
            if trimmed.starts_with("## ") {
                flush(&current_role, &body, &mut out);
                current_role = None;
                body.clear();
            }
            continue;
        }
        if current_role.is_some() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }
    flush(&current_role, &body, &mut out);
    out
}

fn connector_user_assistant(path: &Path, connector: &dyn Connector) -> Vec<(String, String)> {
    let scan_root = if file_name_is(path, "transcript.jsonl") || path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().expect("parent").to_path_buf()
    };
    let tmp = TempDir::new().expect("tempdir");
    let ctx = ScanContext::with_roots(
        tmp.path().join("cass-data"),
        vec![ScanRoot::local(scan_root)],
        None,
    );
    let conversations = connector.scan(&ctx).expect("scan fixture");
    assert_eq!(conversations.len(), 1, "expected one conversation");
    conversations[0]
        .messages
        .iter()
        .filter(|message| matches!(message.role.as_str(), "user" | "assistant"))
        .filter(|message| !message.content.trim().is_empty())
        .map(|message| (message.role.clone(), message.content.clone()))
        .collect()
}

fn file_name_is(path: &Path, expected: &str) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some(expected)
}

fn assert_cm_contract(markdown: &str, user: &str, assistant: &str) {
    assert!(
        has_user_heading(markdown),
        "missing user heading:\n{markdown}"
    );
    assert!(
        has_assistant_heading(markdown),
        "missing assistant heading:\n{markdown}"
    );
    assert_eq!(
        heading_unknown_count(markdown),
        0,
        "export was 100% unknown or contained unknown headings:\n{markdown}"
    );
    let user_pos = markdown.find(user).expect("user sentinel");
    let assistant_pos = markdown.find(assistant).expect("assistant sentinel");
    assert!(
        user_pos < assistant_pos,
        "user sentinel must precede assistant sentinel"
    );
}

#[test]
fn grok_acp_export_recovers_user_and_assistant_without_unknown() {
    let path = fixture("grok/session-acp/updates.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "grok acp export");
    let markdown = stdout(&output);
    assert_cm_contract(&markdown, GROK_USER, GROK_ASSISTANT);
    assert!(
        markdown.contains("GROK_USER_SENTINEL_alpha coalesced")
            || (markdown.contains(GROK_USER) && markdown.contains("coalesced")),
        "adjacent user chunks should coalesce:\n{markdown}"
    );
    assert!(
        markdown.contains(GROK_TOOL),
        "bounded tool context should survive:\n{markdown}"
    );
    assert!(
        !markdown.contains(GROK_BLOB),
        "hook/status blob must not leak:\n{markdown}"
    );
    assert!(
        !markdown.contains(GROK_DUPLICATE),
        "chat_history fallback must not duplicate ACP dialogue:\n{markdown}"
    );
    let unknown_like = markdown
        .lines()
        .filter(|line| line.trim().starts_with("## ") && line.to_ascii_lowercase().contains("hook"))
        .count();
    assert_eq!(
        unknown_like, 0,
        "status events must not become headings:\n{markdown}"
    );
}

#[test]
fn grok_chat_history_fallback_recovers_dialogue_once() {
    let path = fixture("grok/session-fallback/updates.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "grok fallback export");
    let markdown = stdout(&output);
    assert_cm_contract(&markdown, GROK_FALLBACK_USER, GROK_FALLBACK_ASSISTANT);
    assert!(!markdown.contains("GROK_SYNTHETIC_SKIP"));
    let user_turns = ordered_user_assistant(&markdown)
        .into_iter()
        .filter(|(role, _)| role == "user")
        .count();
    assert_eq!(user_turns, 1, "fallback user dialogue must appear once");
}

#[test]
fn grok_export_matches_direct_connector_normalization() {
    let path = fixture("grok/session-acp/updates.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "grok parity export");
    let exported = ordered_user_assistant(&stdout(&output));
    let scanned = connector_user_assistant(&path, &GrokConnector::new());
    let exported_norm: Vec<(String, String)> = exported
        .into_iter()
        .map(|(role, text)| (role, text.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    let scanned_norm: Vec<(String, String)> = scanned
        .into_iter()
        .map(|(role, text)| (role, text.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    assert_eq!(
        exported_norm, scanned_norm,
        "CLI export user/assistant sequence must match GrokConnector::scan"
    );
}

#[test]
fn grok_exact_path_scoping_excludes_sibling_session() {
    let path = fixture("grok/sibling-a/updates.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "grok sibling export");
    let markdown = stdout(&output);
    assert!(markdown.contains("GROK_SIBLING_A_USER"));
    assert!(markdown.contains("GROK_SIBLING_A_ASSISTANT"));
    assert!(!markdown.contains("GROK_SIBLING_B_USER"));
    assert!(!markdown.contains("GROK_SIBLING_B_ASSISTANT"));
}

#[test]
fn grok_malformed_recognized_source_fails_actionably() -> TestResult {
    let path = fixture("grok/session-malformed/updates.jsonl");
    let output = export_markdown(&path);
    assert!(
        !output.status.success(),
        "recognized malformed grok source must not succeed"
    );
    let combined = format!("{}\n{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("grok"),
        "diagnostic must name the connector:\n{combined}"
    );
    assert!(
        combined.contains("session-parse") || combined.contains("could not export"),
        "diagnostic must be actionable:\n{combined}"
    );
    assert!(
        !combined.contains("## unknown") || heading_unknown_count(&stdout(&output)) == 0,
        "must not emit a successful all-unknown transcript"
    );
    Ok(())
}

#[test]
fn letta_export_recovers_user_and_assistant_without_unknown() {
    let path = fixture("letta/agent-export-fixture/conv-export-alpha/transcript.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "letta export");
    let markdown = stdout(&output);
    assert_cm_contract(&markdown, LETTA_USER, LETTA_ASSISTANT);
    assert!(
        markdown.contains(LETTA_REASONING) || !markdown.contains("## unknown"),
        "reasoning may render as assistant or be omitted, but must not be unknown:\n{markdown}"
    );
    assert!(
        markdown.contains(LETTA_TOOL_RESULT) || markdown.contains("## tool"),
        "bounded tool context should follow connector policy:\n{markdown}"
    );
    assert!(
        !markdown.contains(LETTA_BLOB),
        "internal/meta blob must not leak:\n{markdown}"
    );
    assert!(!markdown.contains("LETTA_SIBLING_BETA_USER"));
}

#[test]
fn letta_export_matches_direct_connector_normalization() {
    let path = fixture("letta/agent-export-fixture/conv-export-alpha/transcript.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "letta parity export");
    let exported = ordered_user_assistant(&stdout(&output));
    let scanned = connector_user_assistant(&path, &LettaCodeConnector::new());
    let exported_norm: Vec<(String, String)> = exported
        .into_iter()
        .map(|(role, text)| (role, text.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    let scanned_norm: Vec<(String, String)> = scanned
        .into_iter()
        .map(|(role, text)| (role, text.split_whitespace().collect::<Vec<_>>().join(" ")))
        .collect();
    assert_eq!(
        exported_norm, scanned_norm,
        "CLI export user/assistant sequence must match LettaCodeConnector::scan"
    );
}

#[test]
fn letta_exact_path_scoping_excludes_sibling_transcript() {
    let path = fixture("letta/agent-export-fixture/conv-export-alpha/transcript.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "letta sibling export");
    let markdown = stdout(&output);
    assert!(markdown.contains(LETTA_USER));
    assert!(!markdown.contains("LETTA_SIBLING_BETA_USER"));
    assert!(!markdown.contains("LETTA_SIBLING_BETA_ASSISTANT"));
}

#[test]
fn letta_malformed_recognized_source_fails_actionably() {
    let path = fixture("letta/agent-export-fixture/conv-export-malformed/transcript.jsonl");
    let output = export_markdown(&path);
    assert!(
        !output.status.success(),
        "recognized malformed letta source must not succeed"
    );
    let combined = format!("{}\n{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("letta_code") || combined.contains("letta"),
        "diagnostic must name the connector:\n{combined}"
    );
    assert!(
        combined.contains("session-parse") || combined.contains("could not export"),
        "diagnostic must be actionable:\n{combined}"
    );
}

#[test]
fn downstream_cm_contract_smoke_for_grok_and_letta() {
    for rel in [
        "grok/session-acp/updates.jsonl",
        "letta/agent-export-fixture/conv-export-alpha/transcript.jsonl",
    ] {
        let output = export_markdown(&fixture(rel));
        assert_success(&output, rel);
        let markdown = stdout(&output);
        assert!(has_user_heading(&markdown), "{rel} missing user heading");
        assert!(
            has_assistant_heading(&markdown),
            "{rel} missing assistant heading"
        );
        assert_eq!(
            heading_unknown_count(&markdown),
            0,
            "{rel} must not be 100% unknown:\n{markdown}"
        );
        assert!(
            !markdown.contains("GROK_BLOB_SENTINEL") && !markdown.contains("LETTA_BLOB_SENTINEL"),
            "{rel} leaked a blob sentinel:\n{markdown}"
        );
    }
}

#[test]
fn generic_jsonl_export_regression_unchanged() {
    let path = fixture("generic/session.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "generic jsonl export");
    let markdown = stdout(&output);
    assert_cm_contract(&markdown, GENERIC_USER, GENERIC_ASSISTANT);
}

#[test]
fn codex_nested_payload_export_regression() {
    let path = fixture("codex_nested/session.jsonl");
    let output = export_markdown(&path);
    assert_success(&output, "codex nested export");
    let markdown = stdout(&output);
    assert_cm_contract(&markdown, CODEX_USER, CODEX_ASSISTANT);
}
