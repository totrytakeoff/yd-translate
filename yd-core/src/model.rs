//! 对外输出的数据结构（同时用于 JSON 序列化和缓存）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Example {
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Form {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DictInfo {
    pub word: String,
    pub uk: String,
    pub us: String,
    pub entries: Vec<String>,
    pub forms: Vec<Form>,
    pub examples: Vec<Example>,
    pub web: Vec<String>,
    pub speech: String,
}

impl DictInfo {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.web.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorInfo {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub ok: bool,
    pub query: String,
    pub from: String,
    pub to: String,
    /// dict | translate
    pub mode: String,
    /// youdao-api | youdao-dict | bing | cache
    pub source: String,
    pub translation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dict: Option<DictInfo>,
    pub cached: bool,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorInfo>,
}

impl QueryResult {
    pub fn blank(query: &str, from: &str, to: &str) -> Self {
        QueryResult {
            ok: false,
            query: query.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            mode: "translate".to_string(),
            source: String::new(),
            translation: String::new(),
            dict: None,
            cached: false,
            elapsed_ms: 0,
            error: None,
        }
    }
}
