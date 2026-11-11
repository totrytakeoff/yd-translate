//! 无 appKey 时的网页翻译兜底：Google 的 clients5 端点。
//! 该端点不需要 key，也不像 translate_a/single 那样容易被按 IP 限流。

use serde_json::Value;

use crate::api::{agent, http_error, split_text, ApiError};

const WEB_URL: &str = "https://clients5.google.com/translate_a/t";
const MAX_CHUNK: usize = 4000;

fn google_lang(code: &str) -> &str {
    match code {
        "zh-CHS" => "zh-CN",
        "zh-CHT" => "zh-TW",
        other => other,
    }
}

fn extract(value: &Value) -> String {
    let Some(array) = value.as_array() else {
        return String::new();
    };
    let Some(first) = array.first() else {
        return String::new();
    };
    match first {
        // sl=auto 时返回 [[译文, 检测语言]]
        Value::Array(inner) => inner
            .first()
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .to_string(),
        // 指定源语言时返回 ["译文"] 或 ["行1", "行2"]
        Value::String(text) => {
            if array.len() == 1 {
                text.clone()
            } else {
                array
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        _ => String::new(),
    }
}

pub fn google_translate(text: &str, from: &str, to: &str) -> Result<String, ApiError> {
    let mut out = String::new();
    for chunk in split_text(text, MAX_CHUNK) {
        let url = format!(
            "{}?client=dict-chrome-ex&sl={}&tl={}&q={}",
            WEB_URL,
            crate::url_encode(google_lang(from)),
            crate::url_encode(google_lang(to)),
            crate::url_encode(&chunk)
        );
        let resp = agent().get(&url).call().map_err(http_error)?;
        let value: Value = resp
            .into_json()
            .map_err(|e| ApiError::new("PARSE", &format!("网页翻译解析失败：{e}")))?;
        let piece = extract(&value);
        if piece.is_empty() {
            return Err(ApiError::new("WEB", "网页翻译没有返回结果"));
        }
        out.push_str(&piece);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_language_codes() {
        assert_eq!(google_lang("zh-CHS"), "zh-CN");
        assert_eq!(google_lang("zh-CHT"), "zh-TW");
        assert_eq!(google_lang("en"), "en");
    }

    #[test]
    fn extracts_nested_and_flat() {
        let nested: Value = serde_json::from_str(r#"[["你好","en"]]"#).unwrap();
        assert_eq!(extract(&nested), "你好");
        let flat: Value = serde_json::from_str(r#"["hello"]"#).unwrap();
        assert_eq!(extract(&flat), "hello");
        let lines: Value = serde_json::from_str(r#"["a","b"]"#).unwrap();
        assert_eq!(extract(&lines), "a\nb");
    }
}
