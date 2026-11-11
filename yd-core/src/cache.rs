//! 本地缓存：~/.cache/yd/cache.json，7 天过期，最多 500 条。

use std::collections::BTreeMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::model::QueryResult;

const TTL_SECS: u64 = 7 * 86400;
const MAX_ENTRIES: usize = 500;

#[derive(Serialize, Deserialize, Default)]
struct CacheFile {
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    t: u64,
    v: QueryResult,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn get(key: &str) -> Option<QueryResult> {
    let text = fs::read_to_string(Config::cache_path()).ok()?;
    let cache: CacheFile = serde_json::from_str(&text).ok()?;
    let entry = cache.entries.get(key)?;
    if now().saturating_sub(entry.t) > TTL_SECS {
        return None;
    }
    let mut value = entry.v.clone();
    value.cached = true;
    value.elapsed_ms = 0;
    Some(value)
}

pub fn put(key: &str, value: &QueryResult) {
    let path = Config::cache_path();
    let mut cache: CacheFile = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    cache.entries.insert(
        key.to_string(),
        Entry {
            t: now(),
            v: value.clone(),
        },
    );
    if cache.entries.len() > MAX_ENTRIES {
        let mut items: Vec<(String, Entry)> = cache.entries.into_iter().collect();
        items.sort_by_key(|(_, entry)| entry.t);
        let keep_from = items.len() - MAX_ENTRIES * 4 / 5;
        cache.entries = items.split_off(keep_from).into_iter().collect();
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&cache) {
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

/// 清空缓存（修改凭证后调用，避免继续命中旧的兜底结果）。
pub fn clear() {
    let _ = fs::remove_file(Config::cache_path());
}
