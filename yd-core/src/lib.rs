//! yd-core：有道翻译的核心逻辑（签名、翻译、词典、缓存）。

pub mod api;
pub mod cache;
pub mod config;
pub mod dict;
pub mod model;
pub mod ocr;
pub mod web;

/// 缓存结构版本；升级到不兼容的输出格式时递增。
const CACHE_VERSION: &str = "v1";

use std::process::Command;
use std::time::Instant;

use config::Config;
use model::{ErrorInfo, QueryResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dict,
    Translate,
}

#[derive(Debug, Clone, Default)]
pub struct QueryOptions {
    pub from: Option<String>,
    pub to: Option<String>,
    pub mode: Option<Mode>,
    pub no_cache: bool,
    /// 没有 appKey 时是否回落到 Bing（需要系统里有 trans）。
    pub fallback_bing: bool,
}

fn is_cjk(ch: char) -> bool {
    matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

pub fn detect_lang(text: &str) -> &'static str {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.is_empty() {
        return "en";
    }
    let cjk = chars.iter().filter(|c| is_cjk(**c)).count();
    if cjk as f64 / chars.len() as f64 > 0.2 {
        "zh-CHS"
    } else {
        "en"
    }
}

pub fn looks_like_word(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || t.contains('\n') || t.chars().count() > 30 {
        return false;
    }
    if t.chars().any(|c| ".!?;:,，。！？；：、".contains(c)) {
        return false;
    }
    let cjk = t.chars().filter(|c| is_cjk(*c)).count();
    if cjk > 0 {
        let non_space = t.chars().filter(|c| !c.is_whitespace()).count();
        return non_space == cjk && cjk <= 4;
    }
    t.split_whitespace().count() <= 3
}

fn resolve_langs(text: &str, opts: &QueryOptions) -> (String, String) {
    let mut from = opts.from.clone().unwrap_or_else(|| "auto".to_string());
    let mut to = opts.to.clone().unwrap_or_else(|| "zh-CHS".to_string());
    if from == "auto" {
        from = detect_lang(text).to_string();
    }
    if to == "auto" {
        to = "zh-CHS".to_string();
    }
    if from == to {
        to = if from.starts_with("zh") {
            "en".to_string()
        } else {
            "zh-CHS".to_string()
        };
    }
    (from, to)
}

/// 无 appKey 时的兜底链：先走 Google 网页接口（快），失败再用 trans+Bing。
fn web_fallback(text: &str, from: &str, to: &str) -> Result<(String, &'static str), String> {
    if let Ok(translation) = web::google_translate(text, from, to) {
        return Ok((translation, "google"));
    }

    let target = if to.starts_with("zh") { "zh-CN" } else { to };
    let lang = format!("{}:{}", from, target);
    let output = Command::new("trans")
        .args(["-b", "-e", "bing", &lang, text])
        .output()
        .map_err(|_| "未配置 appKey，Google 网页接口和 trans 都不可用".to_string())?;
    if !output.status.success() {
        return Err("未配置 appKey，兜底翻译全部失败".to_string());
    }
    let translation = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if translation.is_empty() {
        Err("兜底翻译返回空结果".to_string())
    } else {
        Ok((translation, "bing"))
    }
}

pub fn query(text: &str, opts: &QueryOptions) -> QueryResult {
    let start = Instant::now();
    let text = text.trim();
    let (from, to) = resolve_langs(text, opts);
    let mut result = QueryResult::blank(text, &from, &to);

    if text.is_empty() {
        result.error = Some(ErrorInfo {
            code: "INPUT".to_string(),
            message: "查询内容不能为空".to_string(),
        });
        return result;
    }

    let mode = opts.mode.unwrap_or(if looks_like_word(text) {
        Mode::Dict
    } else {
        Mode::Translate
    });

    let cache_key = format!(
        "{}|{}|{}|{}|{}",
        CACHE_VERSION,
        match mode {
            Mode::Dict => "dict",
            Mode::Translate => "translate",
        },
        from,
        to,
        text
    );

    if !opts.no_cache {
        if let Some(hit) = cache::get(&cache_key) {
            return hit;
        }
    }

    if mode == Mode::Dict {
        match dict::lookup(text) {
            Ok(Some(info)) => {
                result.ok = true;
                result.mode = "dict".to_string();
                result.source = "youdao-dict".to_string();
                result.dict = Some(info);
                result.elapsed_ms = start.elapsed().as_millis() as u64;
                if !opts.no_cache {
                    cache::put(&cache_key, &result);
                }
                return result;
            }
            Ok(None) => {}
            Err(err) => {
                // 词典接口不可用时，只要有 key 就继续走翻译。
                if !Config::load().has_credentials() {
                    result.error = Some(ErrorInfo {
                        code: err.code,
                        message: err.message,
                    });
                    result.elapsed_ms = start.elapsed().as_millis() as u64;
                    return result;
                }
            }
        }
    }

    result.mode = "translate".to_string();
    let cfg = Config::load();
    if cfg.has_credentials() {
        match api::translate(text, &from, &to, &cfg) {
            Ok(translation) => {
                result.ok = true;
                result.translation = translation;
                result.source = "youdao-api".to_string();
            }
            Err(err) => {
                result.error = Some(ErrorInfo {
                    code: err.code,
                    message: err.message,
                });
            }
        }
    } else if opts.fallback_bing {
        match web_fallback(text, &from, &to) {
            Ok((translation, source)) => {
                result.ok = true;
                result.translation = translation;
                result.source = source.to_string();
            }
            Err(message) => {
                result.error = Some(ErrorInfo {
                    code: "CONFIG".to_string(),
                    message,
                });
            }
        }
    } else {
        result.error = Some(ErrorInfo {
            code: "CONFIG".to_string(),
            message: "未配置有道 appKey，请运行 `yd --init-config` 后填入凭证".to_string(),
        });
    }

    result.elapsed_ms = start.elapsed().as_millis() as u64;
    if result.ok && !opts.no_cache {
        cache::put(&cache_key, &result);
    }
    result
}

pub(crate) fn url_encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// 有道发音 URL（免 key），lang 形如 en / zh-CHS。
pub fn tts_url(text: &str, lang: &str) -> String {
    let kind = if lang.starts_with("en") { "2" } else { "1" };
    format!(
        "https://dict.youdao.com/dictvoice?audio={}&type={}",
        url_encode(text),
        kind
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chinese() {
        assert_eq!(detect_lang("你好世界"), "zh-CHS");
        assert_eq!(detect_lang("hello world"), "en");
    }

    #[test]
    fn word_heuristics() {
        assert!(looks_like_word("hello"));
        assert!(looks_like_word("苹果"));
        assert!(looks_like_word("look up"));
        assert!(!looks_like_word("今天天气不错"));
        assert!(!looks_like_word("This is a sentence."));
        assert!(!looks_like_word("one two three four five"));
    }
}
