//! Connector-backed raw session export.
//!
//! `cass export --format markdown -- <session-file>` historically parsed each
//! JSONL record with generic `role`/`content` heuristics. Agent-native streams
//! such as Grok Build `updates.jsonl` have neither field, so every record
//! became `## unknown`.
//!
//! This module asks the existing connector for a `NormalizedConversation`
//! before that generic fallback runs. Parsing stays in the connector; this
//! file only detects, scopes, selects, and converts.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::connectors::grok::GrokConnector;
use crate::connectors::{Connector, NormalizedConversation, ScanContext, ScanRoot};
use crate::{CliError, CliErrorKind};

const GROK_UPDATE_METHODS: &[&str] = &["session/update", "_x.ai/session/update"];
const GROK_SESSION_FILES: &[&str] = &["updates.jsonl", "chat_history.jsonl", "summary.json"];
const PEEK_LINE_LIMIT: usize = 32;

/// Adapter claimed by conservative path + content detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawExportAdapter {
    Grok,
}

/// Normalized conversation already converted into the raw exporter's turn JSON.
#[derive(Debug, Clone)]
pub(crate) struct ConnectorBackedExport {
    pub title: Option<String>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub messages: Vec<Value>,
}

/// Try to export through a connector. `Ok(None)` means no supported adapter
/// claimed the path and the generic parser may run.
pub(crate) fn try_connector_backed_export(
    requested_path: &Path,
) -> Result<Option<ConnectorBackedExport>, CliError> {
    let Some(adapter) = detect_adapter(requested_path) else {
        return Ok(None);
    };
    let conversation = normalize_with_adapter(adapter, requested_path)?;
    Ok(Some(conversation_to_export(conversation)))
}

pub(crate) fn detect_adapter(path: &Path) -> Option<RawExportAdapter> {
    if looks_like_grok(path) {
        Some(RawExportAdapter::Grok)
    } else {
        None
    }
}

fn normalize_with_adapter(
    adapter: RawExportAdapter,
    requested_path: &Path,
) -> Result<NormalizedConversation, CliError> {
    match adapter {
        RawExportAdapter::Grok => scan_and_select(RawExportAdapter::Grok, requested_path),
    }
}

fn scan_and_select(
    adapter: RawExportAdapter,
    requested_path: &Path,
) -> Result<NormalizedConversation, CliError> {
    let scan_root = adapter.scan_root(requested_path);
    let connector = adapter.connector();
    let data_dir = std::env::temp_dir().join("cass-raw-session-export-scratch");
    let ctx = ScanContext::with_roots(data_dir, vec![ScanRoot::local(scan_root.clone())], None);
    let conversations = connector.scan(&ctx).map_err(|err| {
        recognized_format_error(
            adapter,
            requested_path,
            &scan_root,
            0,
            format!("connector scan failed: {err}"),
        )
    })?;

    select_requested_conversation(adapter, requested_path, &scan_root, conversations)
}

impl RawExportAdapter {
    fn slug(self) -> &'static str {
        match self {
            Self::Grok => "grok",
        }
    }

    fn scan_root(self, requested_path: &Path) -> PathBuf {
        match self {
            Self::Grok => {
                if requested_path.is_dir() {
                    requested_path.to_path_buf()
                } else {
                    requested_path
                        .parent()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| requested_path.to_path_buf())
                }
            }
        }
    }

    fn connector(self) -> Box<dyn Connector> {
        match self {
            Self::Grok => Box::new(GrokConnector::new()),
        }
    }
}

fn select_requested_conversation(
    adapter: RawExportAdapter,
    requested_path: &Path,
    scan_root: &Path,
    conversations: Vec<NormalizedConversation>,
) -> Result<NormalizedConversation, CliError> {
    let requested = normalize_path(requested_path);
    let matches: Vec<NormalizedConversation> = conversations
        .into_iter()
        .filter(|conversation| conversation_matches(&requested, conversation))
        .collect();

    match matches.len() {
        1 => match matches.into_iter().next() {
            Some(conversation) if !conversation.messages.is_empty() => Ok(conversation),
            Some(_) => Err(recognized_format_error(
                adapter,
                requested_path,
                scan_root,
                1,
                "normalization produced a conversation with zero messages",
            )),
            None => Err(recognized_format_error(
                adapter,
                requested_path,
                scan_root,
                0,
                "matched conversation was dropped after selection",
            )),
        },
        count => Err(recognized_format_error(
            adapter,
            requested_path,
            scan_root,
            count,
            if count == 0 {
                "normalization produced zero conversations for the requested path"
            } else {
                "normalization produced multiple conversations for the requested path"
            }
            .to_string(),
        )),
    }
}

fn conversation_matches(requested: &Path, conversation: &NormalizedConversation) -> bool {
    let source = normalize_path(&conversation.source_path);
    if paths_eq(requested, &source) {
        return true;
    }

    if requested.is_dir() {
        return source.starts_with(requested)
            || source
                .parent()
                .is_some_and(|parent| paths_eq(parent, requested));
    }

    if is_grok_session_sidecar(requested)
        && let (Some(requested_parent), Some(source_parent)) = (requested.parent(), source.parent())
    {
        return paths_eq(requested_parent, source_parent);
    }

    false
}

fn conversation_to_export(conversation: NormalizedConversation) -> ConnectorBackedExport {
    ConnectorBackedExport {
        title: conversation.title,
        started_at: conversation.started_at,
        ended_at: conversation.ended_at,
        messages: conversation
            .messages
            .into_iter()
            .map(|message| {
                json!({
                    "role": message.role,
                    "content": message.content,
                    "author": message.author,
                    "timestamp": message.created_at,
                })
            })
            .collect(),
    }
}

fn recognized_format_error(
    adapter: RawExportAdapter,
    requested_path: &Path,
    scan_root: &Path,
    conversation_count: usize,
    reason: impl Into<String>,
) -> CliError {
    CliError {
        code: 9,
        kind: CliErrorKind::SessionParse.kind_str(),
        message: format!(
            "Recognized {} session at {} but could not export it: {} (scan_root={}, conversations={})",
            adapter.slug(),
            requested_path.display(),
            reason.into(),
            scan_root.display(),
            conversation_count
        ),
        hint: Some(format!(
            "Repair the {} source or export a different session. cass will not fall back to a generic all-unknown transcript for a recognized {} file.",
            adapter.slug(),
            adapter.slug()
        )),
        retryable: false,
    }
}

fn looks_like_grok(path: &Path) -> bool {
    if path.is_dir() {
        let updates = path.join("updates.jsonl");
        if updates.is_file() {
            return file_has_grok_signature(&updates);
        }
        let chat_history = path.join("chat_history.jsonl");
        if chat_history.is_file() {
            return file_has_grok_signature(&chat_history);
        }
        let summary = path.join("summary.json");
        return summary.is_file() && file_has_grok_signature(&summary);
    }

    let name = file_name_str(path);
    if !GROK_SESSION_FILES.contains(&name) {
        return false;
    }
    file_has_grok_signature(path)
}

fn file_has_grok_signature(path: &Path) -> bool {
    if file_name_str(path) == "summary.json" {
        return peek_json_value(path).is_some_and(|value| json_looks_like_grok_summary(&value));
    }
    peek_jsonl_values(path)
        .into_iter()
        .any(|value| json_looks_like_grok_record(&value))
}

fn json_looks_like_grok_record(value: &Value) -> bool {
    if value
        .get("method")
        .and_then(Value::as_str)
        .is_some_and(|method| GROK_UPDATE_METHODS.contains(&method))
    {
        return true;
    }
    if value
        .pointer("/params/update/sessionUpdate")
        .and_then(Value::as_str)
        .is_some()
        || value.get("sessionUpdate").and_then(Value::as_str).is_some()
    {
        return true;
    }
    matches!(
        value.get("type").and_then(Value::as_str),
        Some("user" | "assistant")
    ) && (value.get("prompt_index").is_some()
        || value.get("content").is_some()
        || value.get("synthetic_reason").is_some())
}

fn json_looks_like_grok_summary(value: &Value) -> bool {
    value.pointer("/info/id").and_then(Value::as_str).is_some()
        || value
            .get("generated_title")
            .and_then(Value::as_str)
            .is_some()
        || value
            .get("current_model_id")
            .and_then(Value::as_str)
            .is_some()
        || value
            .get("session_summary")
            .and_then(Value::as_str)
            .is_some()
}

fn is_grok_session_sidecar(path: &Path) -> bool {
    GROK_SESSION_FILES.contains(&file_name_str(path))
}

fn peek_jsonl_values(path: &Path) -> Vec<Value> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for line in BufReader::new(file).lines() {
        if values.len() >= PEEK_LINE_LIMIT {
            break;
        }
        let Ok(line) = line else {
            break;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            values.push(value);
        }
    }
    values
}

fn peek_json_value(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn file_name_str(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn paths_eq(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, body).expect("write fixture");
    }

    fn grok_user_line(text: &str) -> String {
        format!(
            r#"{{"timestamp":1,"method":"session/update","params":{{"sessionId":"sess-1","update":{{"sessionUpdate":"user_message_chunk","content":{{"type":"text","text":"{text}"}}}}}}}}"#
        )
    }

    #[test]
    fn detect_adapter_requires_grok_signature_not_just_basename() {
        let tmp = TempDir::new().expect("tempdir");
        let generic = tmp.path().join("updates.jsonl");
        write(
            &generic,
            r#"{"role":"user","content":"not a grok stream"}
"#,
        );
        assert_eq!(detect_adapter(&generic), None);
    }

    #[test]
    fn detect_adapter_claims_session_update_and_xai_method() {
        let tmp = TempDir::new().expect("tempdir");
        let session = tmp.path().join("session");
        write(
            &session.join("updates.jsonl"),
            &format!("{}\n", grok_user_line("hi")),
        );
        assert_eq!(
            detect_adapter(&session.join("updates.jsonl")),
            Some(RawExportAdapter::Grok)
        );
        assert_eq!(detect_adapter(&session), Some(RawExportAdapter::Grok));

        let xai = tmp.path().join("xai").join("updates.jsonl");
        write(
            &xai,
            r#"{"timestamp":1,"method":"_x.ai/session/update","params":{"sessionId":"sess-1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"ok"}}}}
"#,
        );
        assert_eq!(detect_adapter(&xai), Some(RawExportAdapter::Grok));
    }

    #[test]
    fn select_requested_conversation_rejects_zero_and_ambiguous() {
        let tmp = TempDir::new().expect("tempdir");
        let requested = tmp.path().join("updates.jsonl");
        write(&requested, "{}\n");
        let err = select_requested_conversation(
            RawExportAdapter::Grok,
            &requested,
            tmp.path(),
            Vec::new(),
        )
        .expect_err("zero conversations");
        assert_eq!(err.kind, CliErrorKind::SessionParse.kind_str());
        assert!(err.message.contains("grok"));
        assert!(err.message.contains("conversations=0"));
        assert!(!err.message.contains("hi from the transcript"));
    }

    #[test]
    fn try_connector_backed_export_returns_none_for_generic_jsonl() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("session.jsonl");
        write(
            &path,
            r#"{"role":"user","content":"generic"}
{"role":"assistant","content":"reply"}
"#,
        );
        assert!(
            try_connector_backed_export(&path)
                .expect("generic")
                .is_none()
        );
    }
}
