use super::types::*;
use tokio::time::{timeout, Duration};

const DEFAULT_BASE_URL: &str = "http://localhost:11434";

pub struct OllamaClient {
    base_url: String,
    http: reqwest::Client,
}

impl OllamaClient {
    pub fn new() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Ollama が起動しているか確認（GET /）
    pub async fn health_check(&self) -> Result<bool, String> {
        match self.http.get(&self.base_url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// 利用可能なモデル一覧を取得（GET /api/tags）
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, String> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Ollama への接続に失敗: {e}"))?;
        let tags: TagsResponse = resp
            .json()
            .await
            .map_err(|e| format!("レスポンスのパースに失敗: {e}"))?;
        Ok(tags.models)
    }

    /// 指定モデルが利用可能か確認
    pub async fn has_model(&self, name: &str) -> Result<bool, String> {
        let models = self.list_models().await?;
        // "glm-ocr" で "glm-ocr:latest" にもマッチさせる
        Ok(models.iter().any(|m| {
            m.name == name || m.name.starts_with(&format!("{name}:"))
        }))
    }

    /// Vision モデルに画像を送って結果を取得（POST /api/chat）
    pub async fn chat_vision(
        &self,
        model: &str,
        prompt: &str,
        image_base64: &str,
    ) -> Result<String, String> {
        let url = format!("{}/api/chat", self.base_url);
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                images: Some(vec![image_base64.to_string()]),
            }],
            stream: false,
            keep_alive: Some("3m".to_string()),
            options: Some(ChatOptions {
                num_predict: Some(8192),
                temperature: Some(0.0),
            }),
        };

        let resp = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Ollama OCR リクエスト失敗: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama エラー (HTTP {status}): {body}"));
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .map_err(|e| format!("OCR レスポンスのパースに失敗: {e}"))?;
        Ok(chat_resp.message.content)
    }

    /// テキストのみのチャット（LLM 校正用）
    pub async fn chat_text(
        &self,
        model: &str,
        prompt: &str,
    ) -> Result<String, String> {
        let url = format!("{}/api/chat", self.base_url);
        let request = ChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
                images: None,
            }],
            stream: false,
            keep_alive: Some("3m".to_string()),
        };

        let resp = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Ollama LLM リクエスト失敗: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama エラー (HTTP {status}): {body}"));
        }

        let chat_resp: ChatResponse = resp
            .json()
            .await
            .map_err(|e| format!("LLM レスポンスのパースに失敗: {e}"))?;
        Ok(chat_resp.message.content)
    }

    /// モデルをアンロード（POST /api/generate / keep_alive: 0）。3秒でタイムアウト。
    pub async fn unload_model(&self, model: &str) -> Result<(), String> {
        let url = format!("{}/api/generate", self.base_url);
        let request = GenerateUnloadRequest { model, keep_alive: 0 };
        let fut = self.http.post(&url).json(&request).send();
        let resp = timeout(Duration::from_secs(3), fut)
            .await
            .map_err(|_| "unload timeout".to_string())?
            .map_err(|e| format!("unload リクエスト失敗: {e}"))?;
        let _ = resp.bytes().await;
        Ok(())
    }

    /// モデルをダウンロード（POST /api/pull）。非ストリーミング。
    pub async fn pull_model(&self, model: &str) -> Result<(), String> {
        let url = format!("{}/api/pull", self.base_url);
        let request = PullRequest {
            model: model.to_string(),
            stream: false,
        };

        let resp = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("モデル pull リクエスト失敗: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("モデル pull 失敗 (HTTP {status}): {body}"));
        }

        Ok(())
    }
}
