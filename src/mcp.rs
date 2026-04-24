use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::tools::{self, Context as ToolCtx};

const PROTOCOL_VERSION: &str = "2024-11-05";

pub async fn run_stdio(ctx: ToolCtx) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(%e, raw = %line, "invalid JSON");
                continue;
            }
        };

        let Some(method) = req.get("method").and_then(|v| v.as_str()) else {
            continue;
        };
        let id = req.get("id").cloned();
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        let response = handle(&ctx, method, params).await;

        // Notification (no id, e.g. notifications/initialized)
        if id.is_none() {
            continue;
        }

        let payload = match response {
            Ok(result) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            }),
            Err(err) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32000,
                    "message": err.to_string(),
                }
            }),
        };
        let line = serde_json::to_string(&payload)?;
        stdout.write_all(line.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn handle(ctx: &ToolCtx, method: &str, params: Value) -> Result<Value> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "claude-fetch-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "tools/list" => Ok(tools::tool_list()),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .context("missing tool name")?;
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            tools::dispatch(ctx, name, args).await
        }
        "ping" => Ok(json!({})),
        "shutdown" => Ok(Value::Null),
        other => Err(anyhow::anyhow!("method not implemented: {other}")),
    }
}
