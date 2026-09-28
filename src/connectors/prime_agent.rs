//! CASS adapter for `franken_agent_detection::connectors::prime_agent`.
//!
//! FAD owns Prime parsing and default-root discovery. This wrapper applies
//! CASS ingest contracts that the raw FAD scan does not: remote workspace
//! rewrite, searchable tool-call argument text, and `extra.cass.token_usage`.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Value, json};

use super::{
    Connector, DetectionResult, DiscoveredSourceFile, NormalizedConversation, NormalizedMessage,
    ScanContext, ScanRoot, TokenDataSource, extract_tokens_for_agent,
};

pub struct PrimeAgentConnector {
    inner: franken_agent_detection::PrimeAgentConnector,
}

impl Default for PrimeAgentConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl PrimeAgentConnector {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: franken_agent_detection::PrimeAgentConnector::new(),
        }
    }
}

impl Connector for PrimeAgentConnector {
    fn detect(&self) -> DetectionResult {
        self.inner.detect()
    }

    fn scan(&self, ctx: &ScanContext) -> Result<Vec<NormalizedConversation>> {
        let mut conversations = self.inner.scan(ctx)?;
        enrich_prime_conversations(&mut conversations, ctx);
        Ok(conversations)
    }

    fn discover_source_files(&self, ctx: &ScanContext) -> Result<Vec<DiscoveredSourceFile>> {
        self.inner.discover_source_files(ctx)
    }
}

fn enrich_prime_conversations(conversations: &mut [NormalizedConversation], ctx: &ScanContext) {
    for conversation in conversations {
        apply_workspace_rewrite(conversation, ctx);
        for message in &mut conversation.messages {
            append_invocation_argument_text(message);
            stamp_token_usage(message);
        }
    }
}

fn apply_workspace_rewrite(conversation: &mut NormalizedConversation, ctx: &ScanContext) {
    let Some(workspace) = conversation.workspace.as_ref() else {
        return;
    };
    let original = workspace.to_string_lossy().to_string();
    let Some(root) = matching_rewrite_root(ctx, &conversation.source_path) else {
        return;
    };
    let rewritten = root.rewrite_workspace(&original, Some("prime_agent"));
    if rewritten != original {
        conversation.workspace = Some(PathBuf::from(rewritten));
    }
}

fn matching_rewrite_root<'a>(
    ctx: &'a ScanContext,
    source_path: &std::path::Path,
) -> Option<&'a ScanRoot> {
    ctx.scan_roots
        .iter()
        .find(|root| {
            !root.workspace_rewrites.is_empty()
                && (source_path.starts_with(&root.path) || root.path.starts_with(source_path))
        })
        .or_else(|| {
            ctx.scan_roots
                .iter()
                .find(|root| !root.workspace_rewrites.is_empty())
        })
}

fn append_invocation_argument_text(message: &mut NormalizedMessage) {
    let mut leaves = Vec::new();
    for invocation in &message.invocations {
        if !invocation.name.is_empty() {
            leaves.push(invocation.name.clone());
        }
        if let Some(arguments) = &invocation.arguments {
            collect_string_leaves(arguments, &mut leaves);
        }
    }
    for leaf in leaves {
        if !leaf.is_empty() && !message.content.contains(&leaf) {
            if !message.content.is_empty() {
                message.content.push('\n');
            }
            message.content.push_str(&leaf);
        }
    }
}

fn collect_string_leaves(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => {
            for item in items {
                collect_string_leaves(item, out);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_string_leaves(item, out);
            }
        }
        _ => {}
    }
}

fn stamp_token_usage(message: &mut NormalizedMessage) {
    let extracted = extract_tokens_for_agent(
        "prime_agent",
        &message.extra,
        &message.content,
        &message.role,
    );
    if extracted.data_source != TokenDataSource::Api {
        return;
    }
    if !message.extra.is_object() {
        message.extra = json!({});
    }
    let Some(extra) = message.extra.as_object_mut() else {
        return;
    };
    let cass = extra.entry("cass").or_insert_with(|| json!({}));
    if !cass.is_object() {
        *cass = json!({});
    }
    let Some(cass) = cass.as_object_mut() else {
        return;
    };
    let token_usage = cass.entry("token_usage").or_insert_with(|| json!({}));
    if !token_usage.is_object() {
        *token_usage = json!({});
    }
    if let Some(token_usage) = token_usage.as_object_mut() {
        token_usage.insert(
            "data_source".into(),
            json!(extracted.data_source.as_str()),
        );
    }
}
