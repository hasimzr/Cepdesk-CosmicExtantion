use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const VERSION: u32 = 1;
pub const MAX_MESSAGE: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub v: u32,
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}
impl ApiError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable: false,
        }
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for ApiError {}
pub type Result<T> = std::result::Result<T, ApiError>;
pub fn internal<T>(_: T) -> ApiError {
    ApiError::new(
        "INTERNAL",
        "İşlem tamamlanamadı; servis durumunu kontrol edin.",
    )
}
pub fn response(id: &str, result: Result<Value>) -> Value {
    match result {
        Ok(data) => json!({"v":VERSION,"id":id,"ok":true,"data":data}),
        Err(error) => json!({"v":VERSION,"id":id,"ok":false,"error":error}),
    }
}
pub fn text<'a>(params: &'a Value, key: &str, max: usize) -> Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max)
        .ok_or_else(|| ApiError::new("INVALID_ARGUMENT", &format!("Geçersiz alan: {key}")))
}
#[derive(Clone, Debug)]
pub struct Event {
    pub name: &'static str,
    pub data: Value,
    pub owner: Option<String>,
}
