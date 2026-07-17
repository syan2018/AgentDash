/// OpenAI Responses API 直连 Bridge
///
/// 对标 pi-mono `openai-responses.ts` + `openai-responses-shared.ts`，
/// 直接走 `/v1/responses` SSE 流。不依赖任何 LLM SDK。
use std::pin::Pin;

use async_trait::async_trait;

use agentdash_agent::bridge::{BridgeError, BridgeRequest, LlmBridge, StreamChunk};

use super::openai_responses_common::{
    ResponsesRequestOptions, build_responses_request_body, process_responses_stream,
};

pub struct OpenAiResponsesBridge {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
}

impl OpenAiResponsesBridge {
    pub fn new(api_key: &str, model_id: &str, base_url: Option<&str>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
            api_key: api_key.to_string(),
            model_id: model_id.to_string(),
        }
    }
}

#[async_trait]
impl LlmBridge for OpenAiResponsesBridge {
    async fn stream_complete(
        &self,
        request: BridgeRequest,
    ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
        let client = self.client.clone();
        let url = format!("{}/responses", self.base_url);
        let api_key = self.api_key.clone();
        let model_id = self.model_id.clone();

        super::spawn_bridge_stream(move |tx| async move {
            run_stream(&client, &url, &api_key, &model_id, &request, &tx).await
        })
    }
}

async fn run_stream(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    model_id: &str,
    request: &BridgeRequest,
    tx: &tokio::sync::mpsc::Sender<StreamChunk>,
) -> Result<(), BridgeError> {
    let body =
        build_responses_request_body(model_id, request, ResponsesRequestOptions::openai_api());

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {api_key}"))
        .body(
            serde_json::to_string(&body)
                .map_err(|error| BridgeError::RequestBuildFailed(error.to_string()))?,
        )
        .send()
        .await
        .map_err(|error| super::provider_transport_error("HTTP 请求失败", error))?;

    let response = super::check_http_response(response, "API").await?;

    process_responses_stream(response, "读取响应流失败", tx).await
}
