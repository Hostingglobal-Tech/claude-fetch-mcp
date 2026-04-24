use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::cache::{self, Cache};
use crate::fetch::{self, Client};

pub struct Context {
    pub client: Client,
    pub cache: Arc<Cache>,
}

impl Context {
    pub async fn new() -> Result<Self> {
        let client = Client::new()?;
        let cache = Arc::new(Cache::open()?);
        Ok(Self { client, cache })
    }
}

#[derive(Debug, Deserialize)]
struct FetchArgs {
    url: String,
    #[serde(default)]
    ttl_secs: Option<i64>,
    #[serde(default)]
    bypass_cache: Option<bool>,
    #[serde(default)]
    max_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
struct FetchMeta {
    url: String,
    final_url: String,
    status: u16,
    content_type: String,
    size_bytes: usize,
    network_ms: u128,
    cached: bool,
}

fn default_ttl() -> i64 {
    3600
}

fn cap_body(bytes: &[u8], max: Option<usize>) -> &[u8] {
    let limit = max.unwrap_or(1_000_000);
    if bytes.len() <= limit {
        bytes
    } else {
        &bytes[..limit]
    }
}

pub fn tool_list() -> Value {
    json!({
        "tools": [
            {
                "name": "fetch_url",
                "description": "URL fetch + title/size/snippet meta 반환. LLM 요약 없음. raw body 생략.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string", "description": "http/https URL"},
                        "ttl_secs": {"type": "integer", "description": "캐시 TTL (기본 3600)"},
                        "bypass_cache": {"type": "boolean", "default": false}
                    },
                    "required": ["url"]
                }
            },
            {
                "name": "fetch_markdown",
                "description": "URL → markdown 변환 결과. Claude가 본문을 직접 읽을 때 사용.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "ttl_secs": {"type": "integer"},
                        "bypass_cache": {"type": "boolean"},
                        "max_bytes": {"type": "integer", "description": "반환 markdown 최대 바이트"}
                    },
                    "required": ["url"]
                }
            },
            {
                "name": "fetch_raw",
                "description": "URL raw body. JSON / plaintext / 크롤러 feed 용도.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "url": {"type": "string"},
                        "ttl_secs": {"type": "integer"},
                        "bypass_cache": {"type": "boolean"},
                        "max_bytes": {"type": "integer"}
                    },
                    "required": ["url"]
                }
            },
            {
                "name": "cache_stats",
                "description": "캐시 hit/miss/write/entry-count",
                "inputSchema": {"type": "object"}
            },
            {
                "name": "cache_purge",
                "description": "만료 엔트리 삭제. ttl_secs 미지정 시 전체 미삭제.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ttl_secs": {"type": "integer"}
                    }
                }
            }
        ]
    })
}

pub async fn dispatch(ctx: &Context, name: &str, args: Value) -> Result<Value> {
    match name {
        "fetch_url" => fetch_url(ctx, args).await,
        "fetch_markdown" => fetch_markdown(ctx, args).await,
        "fetch_raw" => fetch_raw(ctx, args).await,
        "cache_stats" => Ok(cache_stats(ctx)),
        "cache_purge" => cache_purge(ctx, args),
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    }
}

async fn fetch_body(
    ctx: &Context,
    args: &FetchArgs,
    variant: &str,
) -> Result<(FetchMeta, Vec<u8>, String)> {
    let ttl = args.ttl_secs.unwrap_or_else(default_ttl);
    let bypass = args.bypass_cache.unwrap_or(false);
    let key = cache::key_for(&args.url, variant);

    if !bypass {
        if let Some(entry) = ctx.cache.get(&key, ttl)? {
            let meta = FetchMeta {
                url: args.url.clone(),
                final_url: args.url.clone(),
                status: entry.status,
                content_type: entry.content_type.clone(),
                size_bytes: entry.body.len(),
                network_ms: 0,
                cached: true,
            };
            let ct = entry.content_type.clone();
            return Ok((meta, entry.body, ct));
        }
    }

    let result = ctx.client.get(&args.url).await?;
    let meta = FetchMeta {
        url: args.url.clone(),
        final_url: result.final_url.clone(),
        status: result.status,
        content_type: result.content_type.clone(),
        size_bytes: result.body.len(),
        network_ms: result.network_ms,
        cached: false,
    };
    ctx.cache.put(
        &key,
        &args.url,
        variant,
        result.status,
        &result.content_type,
        &result.body,
    )?;
    Ok((meta, result.body, result.content_type))
}

async fn fetch_url(ctx: &Context, raw: Value) -> Result<Value> {
    let args: FetchArgs = serde_json::from_value(raw)?;
    let (meta, body, ct) = fetch_body(ctx, &args, "raw").await?;
    let text = if ct.starts_with("text/") || ct.contains("json") || ct.contains("xml") {
        Some(String::from_utf8_lossy(&body).chars().take(500).collect::<String>())
    } else {
        None
    };
    let title = if ct.contains("html") {
        fetch::extract_title(&String::from_utf8_lossy(&body))
    } else {
        None
    };
    Ok(text_content(&json!({
        "meta": meta,
        "title": title,
        "snippet": text,
    })))
}

async fn fetch_markdown(ctx: &Context, raw: Value) -> Result<Value> {
    let args: FetchArgs = serde_json::from_value(raw)?;
    let (meta, body, ct) = fetch_body(ctx, &args, "raw").await?;
    let html = String::from_utf8_lossy(&body).into_owned();
    let markdown = if ct.contains("html") {
        fetch::to_markdown(&html)
    } else {
        html
    };
    let capped = cap_body(markdown.as_bytes(), args.max_bytes);
    let md_out = String::from_utf8_lossy(capped).into_owned();
    Ok(text_content(&json!({
        "meta": meta,
        "markdown": md_out,
        "truncated": capped.len() < markdown.len(),
    })))
}

async fn fetch_raw(ctx: &Context, raw: Value) -> Result<Value> {
    let args: FetchArgs = serde_json::from_value(raw)?;
    let (meta, body, _) = fetch_body(ctx, &args, "raw").await?;
    let capped = cap_body(&body, args.max_bytes);
    let body_str = String::from_utf8_lossy(capped).into_owned();
    Ok(text_content(&json!({
        "meta": meta,
        "body": body_str,
        "truncated": capped.len() < body.len(),
    })))
}

fn cache_stats(ctx: &Context) -> Value {
    let (hits, misses, writes, count) = ctx.cache.stats();
    let total = hits + misses;
    let hit_ratio = if total > 0 {
        hits as f64 / total as f64
    } else {
        0.0
    };
    text_content(&json!({
        "hits": hits,
        "misses": misses,
        "writes": writes,
        "entries": count,
        "hit_ratio": hit_ratio,
    }))
}

fn cache_purge(ctx: &Context, raw: Value) -> Result<Value> {
    let ttl = raw
        .get("ttl_secs")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let n = ctx.cache.purge_expired(ttl)?;
    Ok(text_content(&json!({"purged": n, "ttl_secs": ttl})))
}

fn text_content(payload: &Value) -> Value {
    let text = serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string());
    json!({
        "content": [
            {"type": "text", "text": text}
        ],
        "isError": false
    })
}
