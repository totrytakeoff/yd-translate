//! 有道智云通用 OCR：把图片文字识别成文本。
//! 纯 HTTP 实现，Linux / Windows / macOS 通用。

use base64::Engine;
use serde_json::Value;

use crate::api::{agent, http_error, truncate, ApiError};
use crate::config::Config;

pub const OCR_URL: &str = "https://openapi.youdao.com/ocrapi";
/// 文档要求：编码后 < 5M，建议 ≤ 1M。
const MAX_B64_BYTES: usize = 5 * 1024 * 1024;

pub fn error_message(code: &str) -> &'static str {
    match code {
        "1001" => "无效的 OCR 类型",
        "1002" => "不支持的 OCR 图片类型",
        "1003" => "不支持的 OCR 语言类型",
        "1004" => "识别图片过大（编码后需小于 5M）",
        "1006" => "图片不能为空",
        "1201" => "图片 base64 解码失败",
        "1301" => "OCR 识别失败，请换一张更清晰的图片",
        "1411" => "访问频率受限，请稍后再试",
        "108" => "应用 ID 无效，请检查 appKey",
        "202" => "签名校验失败，请检查 appSecret",
        "110" => "应用没有开通「通用文字识别」服务",
        other => crate::api::error_message(other),
    }
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

fn collect_lines(value: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(regions) = value.pointer("/Result/regions").and_then(|v| v.as_array()) else {
        return lines;
    };
    for region in regions {
        let Some(items) = region.get("lines").and_then(|v| v.as_array()) else {
            continue;
        };
        for line in items {
            if let Some(text) = line.get("text").and_then(|v| v.as_str()) {
                let text = text.trim();
                if !text.is_empty() {
                    lines.push(text.to_string());
                }
            }
        }
    }
    lines
}

/// 解码 data:image/...;base64,xxx 或裸 base64。
pub fn decode_data_url(data: &str) -> Result<Vec<u8>, ApiError> {
    let payload = data.split_once(',').map(|(_, rest)| rest).unwrap_or(data);
    let cleaned: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(cleaned)
        .map_err(|e| ApiError::new("1201", &format!("图片 base64 解码失败：{e}")))
}

/// 直接接收前端 FileReader 产生的 data URL。
pub fn recognize_data_url(data_url: &str, lang: &str, cfg: &Config) -> Result<String, ApiError> {
    let bytes = decode_data_url(data_url)?;
    recognize(&bytes, lang, cfg)
}

/// 识别图片中的文字，按行拼接（换行分隔）。
pub fn recognize(image: &[u8], lang: &str, cfg: &Config) -> Result<String, ApiError> {
    if image.is_empty() {
        return Err(ApiError::new("1006", error_message("1006")));
    }
    if !cfg.has_credentials() {
        return Err(ApiError::new(
            "CONFIG",
            "OCR 需要有道 appKey，请运行 `yd --init-config` 或到 GUI 设置里填写",
        ));
    }

    let img = base64::engine::general_purpose::STANDARD.encode(image);
    if img.len() > MAX_B64_BYTES {
        return Err(ApiError::new("1004", error_message("1004")));
    }

    let salt = now_millis().to_string();
    let curtime = now_secs().to_string();
    let sign = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(
            format!(
                "{}{}{}{}{}",
                cfg.app_key,
                truncate(&img),
                salt,
                curtime,
                cfg.app_secret
            )
            .as_bytes(),
        );
        format!("{:x}", hasher.finalize())
    };

    let pairs: [(&str, &str); 10] = [
        ("img", img.as_str()),
        ("langType", lang),
        ("detectType", "10012"),
        ("imageType", "1"),
        ("appKey", cfg.app_key.as_str()),
        ("salt", salt.as_str()),
        ("sign", sign.as_str()),
        ("signType", "v3"),
        ("curtime", curtime.as_str()),
        ("docType", "json"),
    ];
    let resp = agent()
        .post(OCR_URL)
        .send_form(&pairs)
        .map_err(http_error)?;
    let value: Value = resp
        .into_json()
        .map_err(|e| ApiError::new("PARSE", &format!("OCR 返回解析失败：{e}")))?;

    let code = value
        .get("errorCode")
        .and_then(|v| v.as_str())
        .unwrap_or("-1")
        .to_string();
    if code != "0" {
        return Err(ApiError::new(&code, error_message(&code)));
    }

    let lines = collect_lines(&value);
    if lines.is_empty() {
        return Err(ApiError::new("1301", "没有识别到文字"));
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines() {
        let raw = r#"{"errorCode":"0","Result":{"regions":[
            {"lines":[{"text":"第一行"},{"text":" second "}]},
            {"lines":[{"text":"第三行"}]}
        ]}}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(collect_lines(&value), vec!["第一行", "second", "第三行"]);
    }

    #[test]
    fn empty_result() {
        let value: Value = serde_json::from_str(r#"{"errorCode":"0"}"#).unwrap();
        assert!(collect_lines(&value).is_empty());
    }
}
