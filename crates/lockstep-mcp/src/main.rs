//! lockstep MCP server.
//!
//! Stdio transport, newline-delimited JSON-RPC 2.0. stdout is reserved for
//! protocol traffic — diagnostics go to stderr (`LOCKSTEP_LOG=info` to
//! enable).
//!
//! Tools surfaced:
//!   * `verify_migration` — args `{ paths?: string[], base?: string }`.
//!   * `explain_finding`  — args `{ category: string }`.
//!   * `get_config`       — no args; returns the resolved config.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use lockstep_config::Config;
use lockstep_core::Category;
use lockstep_engine::{run, EngineOptions};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("LOCKSTEP_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    let reader = BufReader::new(stdin.lock());
    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            Ok(_) => continue,
            Err(err) => {
                tracing::error!(?err, "stdin read error; exiting");
                break;
            }
        };
        if let Some(response) = handle_line(&line) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct Request {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

fn handle_line(line: &str) -> Option<String> {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(?e, "bad request");
            return Some(error_response(None, -32700, format!("parse error: {e}")));
        }
    };
    if req.jsonrpc != "2.0" {
        return Some(error_response(
            req.id,
            -32600,
            "jsonrpc must be \"2.0\"".into(),
        ));
    }

    // Notifications (no id) → no response.
    let is_notification = req.id.is_none();
    let result = dispatch(&req.method, &req.params);

    if is_notification {
        return None;
    }
    Some(match result {
        Ok(value) => ok_response(req.id, value),
        Err((code, msg)) => error_response(req.id, code, msg),
    })
}

fn dispatch(method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "lockstep-mcp", "version": env!("CARGO_PKG_VERSION") }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "tools/list" => Ok(json!({ "tools": tools_descriptors() })),
        "tools/call" => handle_tools_call(params),
        _ => Err((-32601, format!("method not found: {method}"))),
    }
}

fn tools_descriptors() -> Vec<Value> {
    vec![
        json!({
            "name": "verify_migration",
            "description": "Verify that touched .ts/.tsx files preserve the syntactic behavior \
                            of their .js counterparts on the default branch. Returns a structured \
                            Report (findings + verdict + counts + summary).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Explicit head-side paths to verify. Omit to check every \
                                        touched .ts/.tsx in the repo."
                    },
                    "base": {
                        "type": "string",
                        "description": "Override the default branch (e.g. `master`). Defaults to \
                                        the value in .lockstep/config.toml."
                    },
                    "repo": {
                        "type": "string",
                        "description": "Repo root. Defaults to cwd."
                    }
                }
            }
        }),
        json!({
            "name": "explain_finding",
            "description": "Return human prose for a Finding category.",
            "inputSchema": {
                "type": "object",
                "required": ["category"],
                "properties": {
                    "category": {
                        "type": "string",
                        "enum": [
                            "kind_mismatch",
                            "token_mismatch",
                            "arity_mismatch",
                            "dropped_statement",
                            "stripped_ts_construct",
                            "parse_error"
                        ]
                    }
                }
            }
        }),
        json!({
            "name": "get_config",
            "description": "Return the resolved .lockstep/config.toml as JSON. Useful when \
                            verify_migration produces unexpected findings.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
    ]
}

#[derive(Debug, Deserialize)]
struct ToolCall {
    name: String,
    #[serde(default)]
    arguments: Value,
}

fn handle_tools_call(params: &Value) -> Result<Value, (i64, String)> {
    let call: ToolCall = serde_json::from_value(params.clone())
        .map_err(|e| (-32602, format!("bad tools/call params: {e}")))?;
    match call.name.as_str() {
        "verify_migration" => tool_verify_migration(&call.arguments),
        "explain_finding" => tool_explain_finding(&call.arguments),
        "get_config" => tool_get_config(&call.arguments),
        other => Err((-32602, format!("unknown tool: {other}"))),
    }
}

fn tool_verify_migration(args: &Value) -> Result<Value, (i64, String)> {
    #[derive(Debug, Deserialize, Default)]
    struct Args {
        #[serde(default)]
        paths: Vec<String>,
        #[serde(default)]
        base: Option<String>,
        #[serde(default)]
        repo: Option<String>,
    }
    let parsed: Args = serde_json::from_value(args.clone())
        .map_err(|e| (-32602, format!("bad arguments: {e}")))?;
    let repo_root = parsed
        .repo
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let config_path = repo_root.join(".lockstep").join("config.toml");
    let mut cfg = Config::load(&config_path).map_err(|e| (-32000, format!("config load: {e}")))?;
    if let Some(b) = parsed.base.clone() {
        cfg.default_branch = b;
    }
    let opts = EngineOptions {
        repo_root,
        base_ref_override: parsed.base,
        explicit_paths: parsed.paths.into_iter().map(PathBuf::from).collect(),
        dump_normalized_to: None,
    };
    let report = run(&cfg, &opts).map_err(|e| (-32000, format!("engine: {e}")))?;
    let report_json =
        serde_json::to_value(&report).map_err(|e| (-32000, format!("serialize report: {e}")))?;
    Ok(tool_content(&report.summary, Some(report_json)))
}

fn tool_explain_finding(args: &Value) -> Result<Value, (i64, String)> {
    #[derive(Debug, Deserialize)]
    struct Args {
        category: String,
    }
    let parsed: Args = serde_json::from_value(args.clone())
        .map_err(|e| (-32602, format!("bad arguments: {e}")))?;
    let c = match parsed.category.as_str() {
        "kind_mismatch" => Category::KindMismatch,
        "token_mismatch" => Category::TokenMismatch,
        "arity_mismatch" => Category::ArityMismatch,
        "dropped_statement" => Category::DroppedStatement,
        "stripped_ts_construct" => Category::StrippedTsConstruct,
        "parse_error" => Category::ParseError,
        other => return Err((-32602, format!("unknown category: {other}"))),
    };
    Ok(tool_content(c.explain(), None))
}

fn tool_get_config(_args: &Value) -> Result<Value, (i64, String)> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config_path = cwd.join(".lockstep").join("config.toml");
    let cfg = Config::load(&config_path).map_err(|e| (-32000, format!("config load: {e}")))?;
    let cfg_json =
        serde_json::to_value(&cfg).map_err(|e| (-32000, format!("serialize config: {e}")))?;
    Ok(tool_content(
        &format!("default_branch: {}", cfg.default_branch),
        Some(cfg_json),
    ))
}

fn tool_content(text: &str, structured: Option<Value>) -> Value {
    let mut content = vec![json!({ "type": "text", "text": text })];
    if let Some(s) = structured {
        // Include the structured payload as a JSON-stringified second item;
        // many MCP clients render the text but inspect raw fields elsewhere.
        content.push(json!({
            "type": "text",
            "text": serde_json::to_string_pretty(&s).unwrap_or_default()
        }));
    }
    json!({ "content": content, "isError": false })
}

fn ok_response(id: Option<Value>, result: Value) -> String {
    serde_json::to_string(&Response {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    })
    .unwrap_or_else(|_| "{}".into())
}

fn error_response(id: Option<Value>, code: i64, msg: String) -> String {
    serde_json::to_string(&Response {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(json!({ "code": code, "message": msg })),
    })
    .unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_returns_capabilities() {
        let resp =
            handle_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#).unwrap();
        assert!(resp.contains("\"protocolVersion\""));
        assert!(resp.contains("lockstep-mcp"));
    }

    #[test]
    fn tools_list_returns_three_tools() {
        let resp = handle_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        assert!(resp.contains("verify_migration"));
        assert!(resp.contains("explain_finding"));
        assert!(resp.contains("get_config"));
    }

    #[test]
    fn explain_finding_returns_prose() {
        let resp = handle_line(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"explain_finding","arguments":{"category":"kind_mismatch"}}}"#,
        )
        .unwrap();
        assert!(resp.contains("AST nodes"));
    }

    #[test]
    fn notification_returns_none() {
        let resp = handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
        assert!(resp.is_none());
    }

    #[test]
    fn malformed_request_returns_parse_error() {
        let resp = handle_line("not json").unwrap();
        assert!(resp.contains("parse error"));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let resp =
            handle_line(r#"{"jsonrpc":"2.0","id":4,"method":"who/dis","params":{}}"#).unwrap();
        assert!(resp.contains("method not found"));
    }
}
