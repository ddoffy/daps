/// MCP (Model Context Protocol) server mode for daps.
///
/// Runs a JSON-RPC 2.0 server over stdio, exposing AWS SSM Parameter Store
/// operations as MCP tools for AI assistants.
use crate::completer::ParameterCompleter;
use rusoto_core::RusotoError;
use rusoto_ssm::GetParameterError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

/// Convert a RusotoError<GetParameterError> into a human-readable message.
/// The upstream rusoto types use the AWS-supplied message verbatim, which is
/// frequently empty (e.g. ParameterNotFound), producing cryptic MCP errors.
fn fmt_get_param_err(path: &str, err: RusotoError<GetParameterError>) -> String {
    match &err {
        RusotoError::Service(GetParameterError::ParameterNotFound(msg)) => {
            if msg.is_empty() {
                format!("Parameter not found: {path}")
            } else {
                format!("Parameter not found ({path}): {msg}")
            }
        }
        RusotoError::Service(GetParameterError::InternalServerError(msg)) => {
            format!("SSM internal server error for {path}: {msg}")
        }
        RusotoError::Service(GetParameterError::InvalidKeyId(msg)) => {
            format!("Invalid KMS key id for {path}: {msg}")
        }
        RusotoError::Service(GetParameterError::ParameterVersionNotFound(msg)) => {
            format!("Parameter version not found for {path}: {msg}")
        }
        other => {
            let s = other.to_string();
            if s.is_empty() {
                format!("Failed to fetch parameter {path}: {other:?}")
            } else {
                format!("Failed to fetch parameter {path}: {s}")
            }
        }
    }
}

// ── JSON-RPC 2.0 wire types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Request {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl Response {
    fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError { code, message: message.into() }),
        }
    }
}

// ── Tool definitions ─────────────────────────────────────────────────────────

fn tool_definitions() -> Value {
    json!({
        "tools": [
            {
                "name": "get_parameter",
                "description": "Fetch a single AWS SSM parameter value by its full path",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Full parameter path, e.g. /prod/db/password" }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "list_parameters",
                "description": "List all cached parameter paths under a given prefix",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path prefix to filter by, e.g. /prod/ (defaults to /)" }
                    },
                    "required": []
                }
            },
            {
                "name": "set_parameter",
                "description": "Update the value of an existing AWS SSM parameter",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path":  { "type": "string", "description": "Full parameter path" },
                        "value": { "type": "string", "description": "New value to set" }
                    },
                    "required": ["path", "value"]
                }
            },
            {
                "name": "insert_parameter",
                "description": "Create a new AWS SSM parameter",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path":  { "type": "string", "description": "Full parameter path" },
                        "value": { "type": "string", "description": "Parameter value" },
                        "type":  { "type": "string", "description": "Parameter type: String, StringList, or SecureString (default: String)" }
                    },
                    "required": ["path", "value"]
                }
            },
            {
                "name": "search_parameters",
                "description": "Fuzzy-search cached parameter keys by keyword",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "term": { "type": "string", "description": "Search term" }
                    },
                    "required": ["term"]
                }
            },
            {
                "name": "refresh_parameters",
                "description": "Re-fetch all parameters under a path prefix from AWS SSM and update the local cache",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path prefix to refresh, e.g. /prod/ (defaults to base path)" }
                    },
                    "required": []
                }
            }
        ]
    })
}

// ── Tool dispatch ─────────────────────────────────────────────────────────────

/// Stringify any error, falling back to Debug when Display is empty.
fn fmt_any_err<E: std::fmt::Display + std::fmt::Debug>(ctx: &str, e: E) -> String {
    let s = e.to_string();
    if s.is_empty() {
        format!("{ctx}: {e:?}")
    } else {
        format!("{ctx}: {s}")
    }
}

async fn call_tool(
    name: &str,
    args: &Value,
    completer: &mut ParameterCompleter,
) -> Result<Value, String> {
    match name {
        "get_parameter" => {
            let path = args["path"].as_str().ok_or("missing 'path'")?;
            let value = completer
                .get_set_value(path)
                .await
                .map_err(|e| fmt_get_param_err(path, e))?;
            Ok(json!({ "path": path, "value": value }))
        }

        "list_parameters" => {
            let prefix = args["path"].as_str().unwrap_or("/");
            let keys: Vec<&str> = completer
                .values
                .keys()
                .filter(|k| k.starts_with(prefix))
                .map(String::as_str)
                .collect();
            Ok(json!({ "parameters": keys }))
        }

        "set_parameter" => {
            let path = args["path"].as_str().ok_or("missing 'path'")?;
            let value = args["value"].as_str().ok_or("missing 'value'")?;
            completer
                .change_value(path, value.to_string())
                .await
                .map_err(|e| fmt_any_err(&format!("set_parameter {path}"), e))?;
            Ok(json!({ "success": true, "path": path }))
        }

        "insert_parameter" => {
            let path = args["path"].as_str().ok_or("missing 'path'")?;
            let value = args["value"].as_str().ok_or("missing 'value'")?;
            let param_type = args["type"].as_str().unwrap_or("String");
            completer
                .set_parameter(path, value.to_string(), Some(param_type.to_string()))
                .await
                .map_err(|e| fmt_any_err(&format!("insert_parameter {path}"), e))?;
            completer
                .update_all(path, value.to_string())
                .await
                .map_err(|e| fmt_any_err(&format!("insert_parameter {path} (cache)"), e))?;
            Ok(json!({ "success": true, "path": path }))
        }

        "search_parameters" => {
            let term = args["term"].as_str().ok_or("missing 'term'")?;
            use fuzzy_matcher::FuzzyMatcher;
            use fuzzy_matcher::skim::SkimMatcherV2;
            let matcher = SkimMatcherV2::default();
            let mut matches: Vec<(i64, &str)> = completer
                .values
                .keys()
                .filter_map(|k| matcher.fuzzy_match(k, term).map(|score| (score, k.as_str())))
                .collect();
            matches.sort_by(|a, b| b.0.cmp(&a.0));
            let keys: Vec<&str> = matches.iter().map(|(_, k)| *k).collect();
            Ok(json!({ "results": keys }))
        }

        "refresh_parameters" => {
            let base = completer.base_path.clone();
            let path = args["path"].as_str().unwrap_or(&base).to_string();
            let results = completer
                .get_set_values(&path)
                .await
                .map_err(|e| fmt_any_err(&format!("refresh_parameters {path}"), e))?;
            Ok(json!({ "refreshed": results.len(), "path": path }))
        }

        other => Err(format!("Unknown tool: {other}")),
    }
}

// ── MCP content helper ────────────────────────────────────────────────────────

fn text_content(value: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": value.to_string() }]
    })
}

// ── Main server loop ──────────────────────────────────────────────────────────

/// Runs the MCP server over stdio (newline-delimited JSON-RPC 2.0).
///
/// Reads one JSON object per line from stdin, processes it, and writes the
/// response to stdout.  Notifications (no `id`) are silently acknowledged.
pub async fn run(completer: &mut ParameterCompleter) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::err(Value::Null, -32700, format!("Parse error: {e}"));
                let mut out = stdout.lock();
                writeln!(out, "{}", serde_json::to_string(&resp)?)?;
                out.flush()?;
                continue;
            }
        };

        let id = req.id.clone().unwrap_or(Value::Null);

        let resp = match req.method.as_str() {
            "initialize" => Response::ok(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "daps", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),

            // Notification — no response needed; fall through and continue
            "notifications/initialized" | "initialized" => {
                continue;
            }

            "tools/list" => Response::ok(id, tool_definitions()),

            "tools/call" => {
                let params = req.params.as_ref().and_then(|p| p.as_object());
                let tool_name = params
                    .and_then(|p| p.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let args = params
                    .and_then(|p| p.get("arguments"))
                    .cloned()
                    .unwrap_or(json!({}));

                match call_tool(tool_name, &args, completer).await {
                    Ok(result) => Response::ok(id, text_content(result)),
                    Err(msg) => Response::err(id, -32603, msg),
                }
            }

            other => Response::err(id, -32601, format!("Method not found: {other}")),
        };

        let mut out = stdout.lock();
        writeln!(out, "{}", serde_json::to_string(&resp)?)?;
        out.flush()?;
    }

    Ok(())
}
