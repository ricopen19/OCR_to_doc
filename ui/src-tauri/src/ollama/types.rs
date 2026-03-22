use serde::{Deserialize, Serialize};

/// POST /api/chat のリクエスト
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

#[derive(Debug, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

/// POST /api/chat のレスポンス（非ストリーミング）
#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub model: String,
    pub message: ResponseMessage,
    pub done: bool,
    pub total_duration: Option<u64>,
    pub eval_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: String,
}

/// GET /api/tags のレスポンス
#[derive(Debug, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub size: Option<u64>,
}

/// POST /api/pull のリクエスト
#[derive(Debug, Serialize)]
pub struct PullRequest {
    pub model: String,
    pub stream: bool,
}

/// POST /api/pull のストリーミングレスポンス（1行ずつ）
#[derive(Debug, Deserialize)]
pub struct PullProgress {
    pub status: String,
    pub digest: Option<String>,
    pub total: Option<u64>,
    pub completed: Option<u64>,
}
