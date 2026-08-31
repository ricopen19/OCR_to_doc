//! OpenAI 互換 `/v1` エンドポイント向けクライアント。
//! 主な用途は同一 PC 上で動く llama.cpp サーバー（`llama-server`）。
//! Ollama も `/v1` を持つが、OCR の既定経路は `client.rs`（ネイティブ `/api/chat`）を使う。

use serde::{Deserialize, Serialize};

const DEFAULT_BASE_URL: &str = "http://localhost:8080";

#[derive(Clone)]
pub struct OpenAiClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    stream: bool,
    max_tokens: i32,
    temperature: f32,
    // llama.cpp のサンプラー拡張。暴走生成（反復ハルシネーション）対策として送る。
    // OpenAI 本家や未対応サーバーは無視する。
    #[serde(skip_serializing_if = "Option::is_none")]
    repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence_penalty: Option<f32>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: Vec<ContentPart<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ContentPart<'a> {
    #[serde(rename = "text")]
    Text { text: &'a str },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

impl OpenAiClient {
    pub fn new(base_url: Option<String>, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url
                .map(|u| u.trim_end_matches('/').to_string())
                .filter(|u| !u.is_empty())
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            api_key: api_key.filter(|k| !k.trim().is_empty()),
            http: reqwest::Client::new(),
        }
    }

    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => req.bearer_auth(key),
            None => req,
        }
    }

    /// `/v1/models` が応答すればサーバーは生きているとみなす。
    pub async fn health_check(&self) -> Result<bool, String> {
        let url = format!("{}/v1/models", self.base_url);
        match self.with_auth(self.http.get(&url)).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// `/v1/models` のモデル id 一覧。llama-server は通常 1 件。
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        let url = format!("{}/v1/models", self.base_url);
        let resp = self
            .with_auth(self.http.get(&url))
            .send()
            .await
            .map_err(|e| format!("llama.cpp サーバーへの接続に失敗: {}", e.without_url()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("llama.cpp エラー (HTTP {status}): {body}"));
        }
        let parsed: ModelsResponse = resp
            .json()
            .await
            .map_err(|e| format!("モデル一覧のパースに失敗: {}", e.without_url()))?;
        Ok(parsed.data.into_iter().map(|m| m.id).collect())
    }

    /// 画像 1 枚を OCR する（`/v1/chat/completions`、非ストリーミング）。
    pub async fn chat_vision(
        &self,
        model: &str,
        prompt: &str,
        image_base64: &str,
    ) -> Result<String, String> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let request = ChatCompletionRequest {
            model,
            messages: vec![Message {
                role: "user",
                content: vec![
                    ContentPart::Text { text: prompt },
                    ContentPart::ImageUrl {
                        image_url: ImageUrl {
                            url: format!("data:image/png;base64,{image_base64}"),
                        },
                    },
                ],
            }],
            stream: false,
            max_tokens: 8192,
            temperature: 0.2,
            repeat_penalty: Some(1.3),
            presence_penalty: Some(0.3),
        };

        let resp = self
            .with_auth(self.http.post(&url).json(&request))
            .send()
            .await
            .map_err(|e| format!("llama.cpp OCR リクエスト失敗: {}", e.without_url()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("llama.cpp エラー (HTTP {status}): {body}"));
        }

        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| format!("OCR レスポンスのパースに失敗: {}", e.without_url()))?;
        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| "OCR レスポンスに choices がありません".to_string())
    }
}
