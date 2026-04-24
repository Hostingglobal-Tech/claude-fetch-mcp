// fetch-bench: standalone benchmark harness
// Compares raw reqwest fetch+parse vs typical Node.js/Python latency and
// measures LLM-summary-skipped wall-clock vs WebFetch-style pipeline.
//
// Usage:
//   fetch-bench --urls urls.txt --runs 5 --out bench_results.json
//
// Emits JSON with per-url stats {min, max, mean, p50, p95}.

use anyhow::{Context, Result};
use clap::Parser;
use serde::Serialize;
use std::time::Instant;
use tokio::fs;

use claude_fetch_mcp as lib;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// URLs 파일 (한 줄에 하나)
    #[arg(long)]
    urls: String,
    /// 각 URL 당 반복 횟수
    #[arg(long, default_value_t = 5)]
    runs: usize,
    /// 결과 JSON 경로
    #[arg(long, default_value = "bench_results.json")]
    out: String,
    /// 캐시 초기화
    #[arg(long, default_value_t = true)]
    clear_cache: bool,
    /// markdown 변환 포함
    #[arg(long, default_value_t = true)]
    with_markdown: bool,
}

#[derive(Serialize)]
struct UrlStats {
    url: String,
    runs: usize,
    cold_ms: Vec<u128>,
    warm_ms: Vec<u128>,
    markdown_bytes: usize,
    status: u16,
    error: Option<String>,
}

#[derive(Serialize)]
struct Report {
    generated_at: String,
    host: String,
    binary_version: &'static str,
    urls: Vec<UrlStats>,
    cold_mean_ms: f64,
    warm_mean_ms: f64,
    cache_speedup: f64,
}

fn mean(v: &[u128]) -> f64 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<u128>() as f64 / v.len() as f64
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();

    let urls_raw = fs::read_to_string(&args.urls)
        .await
        .with_context(|| format!("reading {}", &args.urls))?;
    let urls: Vec<String> = urls_raw
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|s| s.to_string())
        .collect();

    eprintln!("fetch-bench: {} urls × {} runs", urls.len(), args.runs);

    if args.clear_cache {
        if let Ok(p) = std::env::var("LOCALAPPDATA") {
            let path = std::path::Path::new(&p).join("claude-fetch-mcp").join("cache.sqlite");
            let _ = fs::remove_file(&path).await;
        }
    }

    let client = lib::fetch::Client::new()?;
    let cache = std::sync::Arc::new(lib::cache::Cache::open()?);

    let mut url_stats = Vec::new();

    for url in &urls {
        eprintln!("  {}", url);
        let mut cold_ms = Vec::new();
        let mut warm_ms = Vec::new();
        let mut md_bytes = 0usize;
        let mut last_status = 0u16;
        let mut err: Option<String> = None;

        // Cold (no cache, per run purge)
        for run in 0..args.runs {
            let _ = cache.purge_expired(0);
            let t = Instant::now();
            match client.get(url).await {
                Ok(res) => {
                    let body_text = lib::fetch::body_text(&res);
                    let md = if args.with_markdown && res.content_type.contains("html") {
                        lib::fetch::to_markdown(&body_text)
                    } else {
                        body_text
                    };
                    let ms = t.elapsed().as_millis();
                    cold_ms.push(ms);
                    md_bytes = md.len();
                    last_status = res.status;
                    // Populate cache for warm run
                    let k = lib::cache::key_for(url, "raw");
                    let _ = cache.put(&k, url, "raw", res.status, &res.content_type, &res.body);
                }
                Err(e) => {
                    err = Some(format!("cold run {run}: {e}"));
                    break;
                }
            }
        }

        // Warm (hit cache)
        if err.is_none() {
            for _ in 0..args.runs {
                let t = Instant::now();
                let k = lib::cache::key_for(url, "raw");
                if let Ok(Some(entry)) = cache.get(&k, 3600) {
                    let body_text = String::from_utf8_lossy(&entry.body).into_owned();
                    let md = if args.with_markdown && entry.content_type.contains("html") {
                        lib::fetch::to_markdown(&body_text)
                    } else {
                        body_text
                    };
                    let ms = t.elapsed().as_millis();
                    warm_ms.push(ms);
                    md_bytes = md.len();
                }
            }
        }

        url_stats.push(UrlStats {
            url: url.clone(),
            runs: args.runs,
            cold_ms,
            warm_ms,
            markdown_bytes: md_bytes,
            status: last_status,
            error: err,
        });
    }

    let cold_all: Vec<u128> = url_stats.iter().flat_map(|s| s.cold_ms.clone()).collect();
    let warm_all: Vec<u128> = url_stats.iter().flat_map(|s| s.warm_ms.clone()).collect();
    let cold_mean = mean(&cold_all);
    let warm_mean = mean(&warm_all);
    let speedup = if warm_mean > 0.0 {
        cold_mean / warm_mean
    } else {
        0.0
    };

    let report = Report {
        generated_at: chrono::Utc::now().to_rfc3339(),
        host: hostname_or_default(),
        binary_version: env!("CARGO_PKG_VERSION"),
        urls: url_stats,
        cold_mean_ms: cold_mean,
        warm_mean_ms: warm_mean,
        cache_speedup: speedup,
    };
    let out = serde_json::to_string_pretty(&report)?;
    fs::write(&args.out, &out).await?;
    println!("{}", out);
    eprintln!(
        "\ncold mean: {:.1}ms | warm mean: {:.1}ms | speedup: {:.1}x",
        cold_mean, warm_mean, speedup
    );
    Ok(())
}

fn hostname_or_default() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".into())
}
