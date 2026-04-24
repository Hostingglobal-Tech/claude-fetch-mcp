use anyhow::{anyhow, Result};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT};
use std::time::{Duration, Instant};

pub struct FetchResult {
    pub status: u16,
    pub final_url: String,
    pub content_type: String,
    pub body: Vec<u8>,
    pub network_ms: u128,
}

pub struct Client {
    inner: reqwest::Client,
}

impl Client {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                concat!("claude-fetch-mcp/", env!("CARGO_PKG_VERSION")),
            ),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("text/html,application/xhtml+xml,application/json,text/plain,*/*;q=0.8"),
        );
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("ko,en;q=0.8"));

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .http2_prior_knowledge()
            .use_rustls_tls()
            .gzip(true)
            .brotli(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .or_else(|_| {
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .gzip(true)
                    .brotli(true)
                    .timeout(Duration::from_secs(30))
                    .redirect(reqwest::redirect::Policy::limited(5))
                    .build()
            })?;

        Ok(Self { inner })
    }

    pub async fn get(&self, url: &str) -> Result<FetchResult> {
        let parsed = url::Url::parse(url).map_err(|e| anyhow!("invalid url: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(anyhow!("unsupported scheme: {}", parsed.scheme()));
        }

        let start = Instant::now();
        let resp = self.inner.get(url).send().await?;
        let status = resp.status().as_u16();
        let final_url = resp.url().to_string();
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = resp.bytes().await?.to_vec();
        let network_ms = start.elapsed().as_millis();

        Ok(FetchResult {
            status,
            final_url,
            content_type,
            body,
            network_ms,
        })
    }
}

pub fn body_text(result: &FetchResult) -> String {
    String::from_utf8_lossy(&result.body).into_owned()
}

pub fn to_markdown(html: &str) -> String {
    html2md::parse_html(html)
}

pub fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = lower[open..].find('>')? + open + 1;
    let end = lower[start..].find("</title>")? + start;
    let raw = html.get(start..end)?.trim().to_string();
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}
