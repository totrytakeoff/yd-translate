//! 有道词典网页接口（免 key，非官方）。官方 v3 API 已不再返回词典数据。

use serde_json::Value;

use crate::api::{agent, http_error, ApiError};
use crate::model::{DictInfo, Example, Form};

pub const DICT_URL: &str = "https://dict.youdao.com/jsonapi";

fn as_str(value: Option<&Value>) -> &str {
    value.and_then(|v| v.as_str()).unwrap_or("")
}

fn as_array(value: Option<&Value>) -> &[Value] {
    match value.and_then(|v| v.as_array()) {
        Some(arr) => arr.as_slice(),
        None => &[],
    }
}

/// 去掉高亮用的 HTML 标签和实体。
/// 注意只删已知标签：词条里的 `<英>`、`<非正式>` 是有效内容，不能一起吃掉。
fn clean(input: &str) -> String {
    let mut out = input.to_string();
    for tag in [
        "<b>", "</b>", "<i>", "</i>", "<em>", "</em>", "<br/>", "<br />", "<br>",
    ] {
        out = out.replace(tag, "");
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .trim()
        .to_string()
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !item.is_empty() && !list.contains(&item) {
        list.push(item);
    }
}

fn flatten_trs(node: &Value, out: &mut Vec<String>) {
    for item in as_array(Some(node)) {
        for tr in as_array(item.get("tr")) {
            let Some(l) = tr.get("l") else { continue };
            match l.get("i") {
                Some(Value::String(s)) => push_unique(out, clean(s)),
                Some(Value::Array(items)) => {
                    for it in items {
                        let text = match it {
                            Value::String(s) => s.clone(),
                            Value::Object(_) => as_str(it.get("#text")).to_string(),
                            _ => String::new(),
                        };
                        push_unique(out, clean(&text));
                    }
                }
                _ => {}
            }
            if let Some(tran) = l.get("#tran").and_then(|x| x.as_str()) {
                push_unique(out, clean(tran));
            }
        }
    }
}

fn parse(value: &Value, query: &str) -> Option<DictInfo> {
    let mut info = DictInfo {
        word: query.to_string(),
        ..Default::default()
    };

    for key in ["ec", "ce", "simple", "expand_ec"] {
        let Some(node) = value.get(key) else { continue };
        let words: Vec<&Value> = match node.get("word") {
            Some(Value::Array(arr)) => arr.iter().collect(),
            Some(obj @ Value::Object(_)) => vec![obj],
            _ => Vec::new(),
        };
        for word in words {
            if info.word == query {
                if let Some(rp) = word.get("return-phrase") {
                    let text = match rp {
                        Value::String(s) => s.clone(),
                        Value::Object(_) => {
                            as_str(rp.get("l").and_then(|l| l.get("i"))).to_string()
                        }
                        _ => String::new(),
                    };
                    if !text.is_empty() {
                        info.word = text;
                    }
                }
            }
            if info.uk.is_empty() {
                info.uk = as_str(word.get("ukphone")).to_string();
            }
            if info.us.is_empty() {
                info.us = as_str(word.get("usphone")).to_string();
            }
            if info.speech.is_empty() {
                let speech = as_str(word.get("usspeech"));
                info.speech = if speech.is_empty() {
                    as_str(word.get("ukspeech")).to_string()
                } else {
                    speech.to_string()
                };
            }
            if let Some(trs) = word.get("trs") {
                flatten_trs(trs, &mut info.entries);
            }
            for wf in as_array(word.get("wfs")) {
                let Some(inner) = wf.get("wf") else { continue };
                let value = as_str(inner.get("value"));
                if !value.is_empty() {
                    info.forms.push(Form {
                        name: as_str(inner.get("name")).to_string(),
                        value: value.to_string(),
                    });
                }
            }
        }
    }

    if let Some(web) = value
        .get("web_trans")
        .and_then(|x| x.get("web-translation"))
    {
        for item in as_array(Some(web)).iter().take(3) {
            for tr in as_array(item.get("trans")).iter().take(2) {
                if let Some(summary) = tr.get("summary") {
                    for line in as_array(summary.get("line")).iter().take(2) {
                        if let Some(text) = line.as_str() {
                            push_unique(&mut info.web, clean(text));
                        }
                    }
                }
            }
        }
    }

    if let Some(pairs) = value
        .get("blng_sents_part")
        .and_then(|x| x.get("sentence-pair"))
    {
        for pair in as_array(Some(pairs)).iter().take(2) {
            let src = {
                let eng = as_str(pair.get("sentence-eng"));
                if eng.is_empty() {
                    as_str(pair.get("sentence"))
                } else {
                    eng
                }
            };
            let dst = as_str(pair.get("sentence-translation"));
            if !src.is_empty() {
                info.examples.push(Example {
                    src: clean(src),
                    dst: clean(dst),
                });
            }
        }
    }

    if info.is_empty() {
        None
    } else {
        Some(info)
    }
}

/// 查询词条；接口无内容时返回 Ok(None)。
pub fn lookup(text: &str) -> Result<Option<DictInfo>, ApiError> {
    let resp = agent()
        .post(DICT_URL)
        .send_form(&[("q", text)])
        .map_err(http_error)?;
    let value: Value = resp
        .into_json()
        .map_err(|e| ApiError::new("PARSE", &format!("词典接口解析失败：{e}")))?;
    Ok(parse(&value, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_tags() {
        assert_eq!(clean("<b>hello</b> world"), "hello world");
        assert_eq!(
            clean("<英> 把（建筑物）列为文物保护单位"),
            "<英> 把（建筑物）列为文物保护单位"
        );
        assert_eq!(clean("a&nbsp;b"), "a b");
    }

    #[test]
    fn parse_ec_entry() {
        let raw = r#"{"ec":{"word":[{"return-phrase":"hello","ukphone":"həˈləʊ","usphone":"həˈloʊ",
            "trs":[{"tr":[{"l":{"i":["int. 你好"]}}]}],
            "wfs":[{"wf":{"name":"复数","value":"hellos"}}]}]}}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let info = parse(&value, "hello").unwrap();
        assert_eq!(info.word, "hello");
        assert_eq!(info.uk, "həˈləʊ");
        assert_eq!(info.entries, vec!["int. 你好"]);
        assert_eq!(info.forms.len(), 1);
    }
}
