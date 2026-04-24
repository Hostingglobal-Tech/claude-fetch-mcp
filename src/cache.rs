use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Cache {
    conn: Mutex<Connection>,
    hits: AtomicU64,
    misses: AtomicU64,
    writes: AtomicU64,
}

pub struct Entry {
    pub body: Vec<u8>,
    pub content_type: String,
    pub status: u16,
    pub stored_at: i64,
}

fn cache_path() -> PathBuf {
    if let Ok(p) = std::env::var("CLAUDE_FETCH_CACHE") {
        return PathBuf::from(p);
    }
    if cfg!(windows) {
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            let mut p = PathBuf::from(appdata);
            p.push("claude-fetch-mcp");
            let _ = std::fs::create_dir_all(&p);
            p.push("cache.sqlite");
            return p;
        }
    }
    let mut p = dirs_home();
    p.push(".cache");
    p.push("claude-fetch-mcp");
    let _ = std::fs::create_dir_all(&p);
    p.push("cache.sqlite");
    p
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn key_for(url: &str, variant: &str) -> String {
    let mut h = Sha256::new();
    h.update(url.as_bytes());
    h.update(b"\x1f");
    h.update(variant.as_bytes());
    hex::encode(h.finalize())
}

impl Cache {
    pub fn open() -> Result<Self> {
        let path = cache_path();
        tracing::info!(cache = %path.display(), "cache opening");
        let conn = Connection::open(&path).context("open cache db")?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS entries (
                 key TEXT PRIMARY KEY,
                 url TEXT NOT NULL,
                 variant TEXT NOT NULL,
                 status INTEGER NOT NULL,
                 content_type TEXT NOT NULL,
                 body BLOB NOT NULL,
                 stored_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_entries_url ON entries(url);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            writes: AtomicU64::new(0),
        })
    }

    pub fn get(&self, key: &str, ttl_secs: i64) -> Result<Option<Entry>> {
        let guard = self.conn.lock().unwrap();
        let row = guard
            .query_row(
                "SELECT body, content_type, status, stored_at FROM entries WHERE key=?1",
                params![key],
                |r| {
                    Ok(Entry {
                        body: r.get(0)?,
                        content_type: r.get(1)?,
                        status: r.get::<_, i64>(2)? as u16,
                        stored_at: r.get(3)?,
                    })
                },
            )
            .ok();

        let Some(entry) = row else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        };

        if ttl_secs > 0 && now_unix() - entry.stored_at > ttl_secs {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return Ok(None);
        }

        self.hits.fetch_add(1, Ordering::Relaxed);
        Ok(Some(entry))
    }

    pub fn put(
        &self,
        key: &str,
        url: &str,
        variant: &str,
        status: u16,
        content_type: &str,
        body: &[u8],
    ) -> Result<()> {
        let guard = self.conn.lock().unwrap();
        guard.execute(
            "INSERT OR REPLACE INTO entries (key, url, variant, status, content_type, body, stored_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                key,
                url,
                variant,
                status as i64,
                content_type,
                body,
                now_unix(),
            ],
        )?;
        self.writes.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    pub fn stats(&self) -> (u64, u64, u64, usize) {
        let guard = self.conn.lock().unwrap();
        let count: usize = guard
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .unwrap_or(0);
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
            self.writes.load(Ordering::Relaxed),
            count,
        )
    }

    pub fn purge_expired(&self, ttl_secs: i64) -> Result<usize> {
        if ttl_secs <= 0 {
            return Ok(0);
        }
        let guard = self.conn.lock().unwrap();
        let cutoff = now_unix() - ttl_secs;
        let n = guard.execute("DELETE FROM entries WHERE stored_at < ?1", params![cutoff])?;
        Ok(n)
    }
}
