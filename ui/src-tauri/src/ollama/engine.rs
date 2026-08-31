//! OCR バックエンドの抽象化。
//!
//! 既定は Ollama（ネイティブ `/api/chat`、`client.rs`）。パワーユーザー向けに
//! 同一 PC 上の llama.cpp サーバー（OpenAI 互換 `/v1`、`openai_client.rs`）も選べる。
//! パイプラインは [`OcrBackend`] だけを見て、エンジン差はこのモジュールに閉じ込める。

use serde::{Deserialize, Serialize};

use super::client::OllamaClient;
use super::openai_client::OpenAiClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OcrEngine {
    #[default]
    Ollama,
    LlamaCpp,
}

impl OcrEngine {
    /// 設定ファイル / CLI 引数など境界での文字列を enum に寄せる。
    /// 未知の値は既定（Ollama）にフォールバックする。
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).unwrap_or("") {
            "llamacpp" | "llama.cpp" | "llama_cpp" | "llama-cpp" => Self::LlamaCpp,
            _ => Self::Ollama,
        }
    }

    /// 設定ファイル / フロントエンドで使う識別子。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::LlamaCpp => "llamacpp",
        }
    }
}

/// 既定の OCR モデル名。設定が空 / 未指定のときのフォールバック。
pub const DEFAULT_OCR_MODEL: &str = "glm-ocr";

/// 設定値（空文字・None を含みうる）を実際に使うモデル名に解決する。
pub fn resolve_ocr_model(raw: Option<String>) -> String {
    raw.map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| DEFAULT_OCR_MODEL.to_string())
}

/// エンジンと接続先。境界でパースし、以降は信頼する。
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub engine: OcrEngine,
    /// llama.cpp のベース URL（None なら既定 http://localhost:8080）。
    /// Ollama では未使用。
    pub base_url: Option<String>,
    /// llama.cpp が認証を要求する場合のみ。Bearer ヘッダーで送る。
    pub api_key: Option<String>,
}

impl BackendConfig {
    pub fn new(
        engine: OcrEngine,
        base_url: Option<String>,
        api_key: Option<String>,
    ) -> Self {
        Self { engine, base_url, api_key }
    }

    pub fn ollama_default() -> Self {
        Self { engine: OcrEngine::Ollama, base_url: None, api_key: None }
    }
}

/// エンジン別クライアントのラッパ。
pub enum OcrBackend {
    Ollama(OllamaClient),
    LlamaCpp(OpenAiClient),
}

impl OcrBackend {
    pub fn new(cfg: &BackendConfig) -> Self {
        match cfg.engine {
            // Ollama は常に既定の localhost:11434 を使う。`base_url` / `api_key` は
            // llama.cpp 用の設定であり、呼び出し元がエンジンに関係なく渡してくるため
            // ここで無視する（渡すと Ollama クライアントが llama.cpp サーバーに
            // `/api/tags` を投げてパース失敗する）。
            OcrEngine::Ollama => Self::Ollama(OllamaClient::new()),
            OcrEngine::LlamaCpp => Self::LlamaCpp(OpenAiClient::new(
                cfg.base_url.clone(),
                cfg.api_key.clone(),
            )),
        }
    }

    pub async fn health_check(&self) -> Result<bool, String> {
        match self {
            Self::Ollama(c) => c.health_check().await,
            Self::LlamaCpp(c) => c.health_check().await,
        }
    }

    /// サーバー未起動時にユーザーへ出す案内。
    pub fn not_running_hint(&self) -> String {
        match self {
            Self::Ollama(_) => {
                "Ollama が起動していません。Ollama を起動してください。".to_string()
            }
            Self::LlamaCpp(_) => {
                "llama.cpp サーバーに接続できません。llama-server を起動して、設定の接続先 URL を確認してください。"
                    .to_string()
            }
        }
    }

    /// 指定モデルが使える状態か確認する。
    /// Ollama は `/api/tags` に該当モデルがあるかを見る。
    /// llama.cpp は起動時にロード済みの単一モデルで動くため、名前照合はせず
    /// サーバーが応答するかだけを確認する（名前の食い違いで誤ブロックしない）。
    pub async fn ensure_model(&self, model: &str) -> Result<(), String> {
        match self {
            Self::Ollama(c) => {
                if c.has_model(model).await? {
                    Ok(())
                } else {
                    Err(format!(
                        "OCR モデル '{model}' が見つかりません。'ollama pull {model}' を実行してください。"
                    ))
                }
            }
            Self::LlamaCpp(c) => {
                if c.health_check().await? {
                    Ok(())
                } else {
                    Err(self.not_running_hint())
                }
            }
        }
    }

    pub async fn chat_vision(
        &self,
        model: &str,
        prompt: &str,
        image_base64: &str,
    ) -> Result<String, String> {
        match self {
            Self::Ollama(c) => c.chat_vision(model, prompt, image_base64).await,
            Self::LlamaCpp(c) => c.chat_vision(model, prompt, image_base64).await,
        }
    }

    /// モデル選択 UI 用の一覧。
    pub async fn list_models(&self) -> Result<Vec<String>, String> {
        match self {
            Self::Ollama(c) => {
                Ok(c.list_models().await?.into_iter().map(|m| m.name).collect())
            }
            Self::LlamaCpp(c) => c.list_models().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OcrEngine;

    #[test]
    fn parse_maps_known_llamacpp_aliases() {
        for raw in ["llamacpp", "llama.cpp", "llama-cpp", "  llamacpp  "] {
            assert_eq!(OcrEngine::parse(Some(raw)), OcrEngine::LlamaCpp, "{raw}");
        }
    }

    #[test]
    fn parse_falls_back_to_ollama_for_unknown_or_missing() {
        assert_eq!(OcrEngine::parse(None), OcrEngine::Ollama);
        assert_eq!(OcrEngine::parse(Some("")), OcrEngine::Ollama);
        assert_eq!(OcrEngine::parse(Some("ollama")), OcrEngine::Ollama);
        assert_eq!(OcrEngine::parse(Some("vllm")), OcrEngine::Ollama);
    }

    #[test]
    fn as_str_roundtrips_through_parse() {
        for e in [OcrEngine::Ollama, OcrEngine::LlamaCpp] {
            assert_eq!(OcrEngine::parse(Some(e.as_str())), e);
        }
    }
}
