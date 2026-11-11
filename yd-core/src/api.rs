//! 有道智云文本翻译 API（v3 签名）。

use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::config::Config;

pub const API_URL: &str = "https://openapi.youdao.com/api";
const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
/// 有道限制 q ≤ 5000 字节，留一点余量。
const MAX_CHUNK_BYTES: usize = 4500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &str, message: &str) -> Self {
        ApiError {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}（错误码 {}）", self.message, self.code)
    }
}

impl std::error::Error for ApiError {}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()
}

pub fn http_error(e: ureq::Error) -> ApiError {
    match e {
        ureq::Error::Status(code, _) => ApiError::new("HTTP", &format!("接口返回 HTTP {code}")),
        ureq::Error::Transport(t) => ApiError::new("NET", &format!("网络错误：{t}")),
    }
}

pub fn error_message(code: &str) -> &'static str {
    match code {
        "0" => "成功",
        "101" => "缺少必填参数",
        "102" => "不支持的语言类型",
        "103" => "翻译文本过长",
        "104" => "不支持的 API 类型",
        "105" => "不支持的签名类型",
        "106" => "不支持的响应类型",
        "107" => "不支持的传输加密类型",
        "108" => "应用 ID 无效，请检查 appKey",
        "110" => "无相关翻译服务，请确认应用已开通「文本翻译」",
        "111" => "开发者账号无效",
        "112" => "请求服务无效",
        "113" => "查询内容不能为空",
        "202" => "签名校验失败，请检查 appSecret 是否完整",
        "203" => "访问 IP 不在白名单内",
        "301" => "词典查询失败",
        "302" => "翻译查询失败",
        "303" => "服务端异常",
        "401" => "账户已欠费",
        "402" => "账户已关闭",
        "403" => "账户未实名认证",
        "411" => "访问频率受限，请稍后再试",
        "412" => "长请求过于频繁，请稍后再试",
        "501" => "不支持的语言对",
        "502" => "输入语言错误",
        _ => "未知错误",
    }
}

/// 有道 v3 签名规则：长度 ≤ 20 原样，否则 前10 + 字符数 + 后10。
pub(crate) fn truncate(q: &str) -> String {
    let chars: Vec<char> = q.chars().collect();
    if chars.len() <= 20 {
        q.to_string()
    } else {
        let head: String = chars[..10].iter().collect();
        let tail: String = chars[chars.len() - 10..].iter().collect();
        format!("{}{}{}", head, chars.len(), tail)
    }
}

fn sign(app_key: &str, q: &str, salt: &str, curtime: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}{}{}{}{}", app_key, truncate(q), salt, curtime, secret).as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn translate_once(q: &str, from: &str, to: &str, cfg: &Config) -> Result<String, ApiError> {
    let salt = now_millis().to_string();
    let curtime = now_secs().to_string();
    let sign = sign(&cfg.app_key, q, &salt, &curtime, &cfg.app_secret);
    let pairs: [(&str, &str); 8] = [
        ("q", q),
        ("from", from),
        ("to", to),
        ("appKey", cfg.app_key.as_str()),
        ("salt", salt.as_str()),
        ("sign", sign.as_str()),
        ("signType", "v3"),
        ("curtime", curtime.as_str()),
    ];
    let resp = agent()
        .post(API_URL)
        .send_form(&pairs)
        .map_err(http_error)?;
    let value: serde_json::Value = resp
        .into_json()
        .map_err(|e| ApiError::new("PARSE", &format!("接口返回解析失败：{e}")))?;

    let code = value
        .get("errorCode")
        .and_then(|x| x.as_str())
        .unwrap_or("-1")
        .to_string();
    if code != "0" {
        return Err(ApiError::new(&code, error_message(&code)));
    }

    let translation = value
        .get("translation")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str())
                .collect::<String>()
        })
        .unwrap_or_default();

    if translation.is_empty() {
        return Err(ApiError::new("-1", "接口没有返回译文"));
    }
    Ok(translation)
}

/// 长文本按句切分后再逐段翻译。
pub(crate) fn split_text(text: &str, limit: usize) -> Vec<String> {
    if text.len() <= limit {
        return vec![text.to_string()];
    }

    let mut pieces: Vec<String> = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        buf.push(ch);
        if matches!(ch, '。' | '！' | '？' | '!' | '?' | '.' | '\n') {
            pieces.push(std::mem::take(&mut buf));
        }
    }
    if !buf.is_empty() {
        pieces.push(buf);
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for piece in pieces {
        if !current.is_empty() && current.len() + piece.len() > limit {
            chunks.push(std::mem::take(&mut current));
        }
        if piece.len() > limit {
            let mut rest = piece.as_str();
            while rest.len() > limit {
                let mut cut = limit;
                while !rest.is_char_boundary(cut) {
                    cut -= 1;
                }
                chunks.push(rest[..cut].to_string());
                rest = &rest[cut..];
            }
            current.push_str(rest);
        } else {
            current.push_str(&piece);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

pub fn translate(q: &str, from: &str, to: &str, cfg: &Config) -> Result<String, ApiError> {
    let mut out = String::new();
    for chunk in split_text(q, MAX_CHUNK_BYTES) {
        out.push_str(&translate_once(&chunk, from, to, cfg)?);
    }
    Ok(out)
}

/// 校验凭证是否可用，成功返回一次测试翻译。
pub fn check(cfg: &Config) -> Result<String, ApiError> {
    if !cfg.has_credentials() {
        return Err(ApiError::new(
            "CONFIG",
            "还没配置 appKey / appSecret，运行 `yd --init-config` 生成模板",
        ));
    }
    translate_once("hello", "en", "zh-CHS", cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("hello"), "hello");
    }

    #[test]
    fn truncate_long() {
        assert_eq!(truncate("123456789012345678901"), "1234567890212345678901");
    }

    #[test]
    fn chunking_keeps_content() {
        let text = "你好。".repeat(2000);
        let chunks = split_text(&text, 100);
        assert!(chunks.iter().all(|c| c.len() <= 100));
        assert_eq!(chunks.concat(), text);
    }
}
