//! 基于 rig 的 LLM Provider 实现
//!
//! 封装 rig crate 的调用，支持 OpenAI、Anthropic 等 provider。
//!
//! ## OpenAI 兼容服务
//!
//! `new_openai` 使用 rig 的 Chat Completions API（`CompletionsClient`），
//! 这是 OpenAI 官方以及绝大多数 OpenAI 兼容服务（如 vLLM、ollama、
//! 第三方网关等）普遍支持的接口。`base_url` 可选，缺省时使用 rig 内置
//! 的 `https://api.openai.com/v1`。

use async_trait::async_trait;
use rig::client::CompletionClient;
use rig::completion::{AssistantContent, CompletionModel, CompletionRequest, Message};

use super::LlmProvider;
use crate::error::EvalError;

/// 基于 rig 的 LLM Provider
///
/// 内部使用枚举区分 OpenAI / Anthropic 客户端，
/// 对外统一实现 [`LlmProvider`] trait。
pub struct RigProvider {
    /// Provider 逻辑名称
    name: String,
    /// 默认模型名
    model: String,
    /// 封装的 rig 客户端
    client: RigClient,
}

/// 内部客户端枚举
enum RigClient {
    /// OpenAI Chat Completions API 兼容客户端
    OpenAI(rig::providers::openai::CompletionsClient),
    /// Anthropic Messages API 客户端
    Anthropic(rig::providers::anthropic::Client),
}

impl RigProvider {
    /// 创建 OpenAI 兼容 provider
    ///
    /// # Arguments
    ///
    /// * `api_key` - API 密钥
    /// * `model` - 默认模型名（如 `gpt-4o`、`glm-5.2`）
    /// * `base_url` - 可选的 API 基础 URL，用于 OpenAI 兼容服务。
    ///   为 `None` 时使用 rig 默认值（`https://api.openai.com/v1`）。
    pub fn new_openai(
        api_key: String,
        model: String,
        base_url: Option<String>,
    ) -> Result<Self, EvalError> {
        let mut builder = rig::providers::openai::CompletionsClient::builder().api_key(&api_key);
        if let Some(url) = base_url {
            builder = builder.base_url(url);
        }
        let client = builder
            .build()
            .map_err(|e| EvalError::LlmCallError(format!("创建 OpenAI 兼容 client 失败: {e}")))?;

        Ok(Self {
            name: "openai".to_string(),
            model,
            client: RigClient::OpenAI(client),
        })
    }

    /// 创建 Anthropic provider
    ///
    /// # Arguments
    ///
    /// * `api_key` - Anthropic API 密钥
    /// * `model` - 模型名（如 `claude-3.5-sonnet`）
    pub fn new_anthropic(api_key: String, model: String) -> Result<Self, EvalError> {
        let client = rig::providers::anthropic::Client::new(&api_key)
            .map_err(|e| EvalError::LlmCallError(format!("创建 Anthropic client 失败: {e}")))?;

        Ok(Self {
            name: "anthropic".to_string(),
            model,
            client: RigClient::Anthropic(client),
        })
    }

    /// 构造 CompletionRequest
    ///
    /// temperature=0 确保评测一致性，max_tokens=1024 提供足够输出空间。
    fn build_request(&self, prompt: &str) -> CompletionRequest {
        CompletionRequest {
            model: None,
            preamble: None,
            chat_history: rig::OneOrMany::one(Message::user(prompt)),
            documents: vec![],
            tools: vec![],
            temperature: Some(0.0),
            max_tokens: Some(1024),
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        }
    }

    /// 从 CompletionResponse 中提取文本内容
    fn extract_text(response: rig::completion::CompletionResponse<impl Sized>) -> Result<String, EvalError> {
        for content in response.choice.iter() {
            if let AssistantContent::Text(text) = content {
                return Ok(text.text.clone());
            }
        }
        Err(EvalError::LlmCallError("模型返回空响应".to_string()))
    }
}

#[async_trait]
impl LlmProvider for RigProvider {
    fn name(&self) -> &str {
        &self.name
    }

    /// 发送 completion 请求并提取文本
    async fn complete(&self, prompt: &str, _model: &str) -> Result<String, EvalError> {
        let request = self.build_request(prompt);

        match &self.client {
            RigClient::OpenAI(client) => {
                let model = client.completion_model(&self.model);
                let response = model
                    .completion(request)
                    .await
                    .map_err(|e| EvalError::LlmCallError(e.to_string()))?;

                Self::extract_text(response)
            }
            RigClient::Anthropic(client) => {
                let model = client.completion_model(&self.model);
                let response = model
                    .completion(request)
                    .await
                    .map_err(|e| EvalError::LlmCallError(e.to_string()))?;

                Self::extract_text(response)
            }
        }
    }
}