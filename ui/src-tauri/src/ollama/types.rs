use serde::{Deserialize, Serialize};

/// POST /api/chat の推論オプション
#[derive(Debug, Serialize)]
pub struct ChatOptions {
    /// 最大生成トークン数。stop token のないモデルの暴走を防ぐ。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// 直近トークンの再出現に対するペナルティ。greedy decoding (temperature=0) は
    /// これが未設定だと座標付き要素や数式のような反復パターンで自己ループしやすいため設定する。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// repeat_penalty が遡って見るトークン数。bbox 座標付き要素の1サイクルは
    /// 数十トークンに及ぶため、デフォルト(64)では反復検出の窓が狭すぎることがある。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_last_n: Option<i32>,
    /// 既出トークンへの一律ペナルティ。repeat_penalty と異なり出現回数で強まらないため、
    /// 単純な繰り返しループの抑制に副作用（低確率トークンへの逃避によるハルシネーション）が出にくい。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
}

/// POST /api/chat のリクエスト
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    /// thinking 対応モデルの思考出力を抑制する。OCR では思考ブロックが
    /// 本文に混入して悪さをするため常に false を送る（非対応モデルは無視する）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<bool>,
    /// モデルのメモリ保持時間。"3m" = 3分後に自動アンロード。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ChatOptions>,
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
    pub message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
pub struct ResponseMessage {
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
}

/// POST /api/generate — keep_alive: 0 でモデルをアンロード
#[derive(Debug, Serialize)]
pub struct GenerateUnloadRequest<'a> {
    pub model: &'a str,
    pub keep_alive: i64,
}
