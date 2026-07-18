//! Provider-neutral summarization primitive.
//!
//! The managed Runtime owns policy, checkpoint identity, persistence, and activation. This module
//! only performs deterministic cut selection and an optional LLM summary request.

use tokio_util::sync::CancellationToken;

use crate::bridge::{BridgeRequest, LlmBridge, StreamChunk};
use crate::types::{
    AgentError, AgentMessage, CompactionMetadata, CompactionParams, CompactionResult, ContentPart,
    MessageRef, estimate_message_tokens,
};

/// 默认摘要 system prompt
const SUMMARIZATION_SYSTEM_PROMPT: &str = "\
你是一个会话摘要助手。你只能总结提供的消息，不能继续对话、回答用户、执行任务、提出新的问题或声称已经完成后续动作。请生成一份可供后续模型请求继续推理的结构化摘要。

摘要必须包含：
- 当前目标 / 用户主要意图
- 已完成的主要工作和当前进展
- 做出的关键决策、原因和仍需遵守的约束
- 文件、工具与外部产物的使用状态
- 遇到的错误、失败尝试和已经应用的修复
- 待办事项和最直接的下一步
- 重要的技术发现（文件路径、函数名等具体信息）

要求：
- 摘要是 continuation handoff，不是用户可见回复
- 不要要求用户确认摘要，不要继续当前对话
- 保留所有具体的文件路径、函数名、变量名
- 保留所有未完成的工作和待办事项
- 使用结构化的 markdown 格式
- 简洁但不遗漏关键信息";

/// 迭代式摘要更新 prompt（与前次摘要合并）
const UPDATE_SUMMARIZATION_PROMPT: &str = "\
你是一个会话交接摘要助手。你只能总结上下文，不能继续对话、回答用户、执行任务、提出新的问题或声称已经完成后续动作。你将收到一份已有摘要和一段新的对话历史。

请将新对话中的信息合并到已有摘要中：
- 保留已有摘要中仍然相关的信息
- 添加新对话中的新信息
- 更新已变更的状态
- 删除已过时的信息
- 覆盖当前目标、进展、关键决策、约束、文件/工具状态、错误修复、待办和下一步

输出更新后的完整摘要（markdown 格式）。摘要是 continuation handoff，不是用户可见回复；不要要求用户确认摘要，不要继续当前对话。";

pub fn default_auto_compaction_metadata() -> CompactionMetadata {
    CompactionMetadata::auto_token_pressure_pre_provider()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionEligibility {
    Eligible,
    NoEligibleMessages {
        message_count: usize,
        keep_last_n: u32,
    },
    InvalidInput {
        failure: CompactionEligibilityFailure,
        message_count: usize,
        ref_count: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionEligibilityFailure {
    ContextEmpty,
    MessageRefLengthMismatch,
    CompactedUntilRefMissing,
    FirstKeptRefMissing,
}

impl CompactionEligibilityFailure {
    pub fn reason_code(self) -> &'static str {
        match self {
            Self::ContextEmpty => "compaction_context_empty",
            Self::MessageRefLengthMismatch => "compaction_message_ref_len_mismatch",
            Self::CompactedUntilRefMissing => "compaction_boundary_ref_missing",
            Self::FirstKeptRefMissing => "compaction_first_kept_ref_missing",
        }
    }
}

pub fn evaluate_compaction_eligibility(
    messages: &[AgentMessage],
    message_refs: &[Option<MessageRef>],
    params: &CompactionParams,
) -> CompactionEligibility {
    let message_count = messages.len();
    let ref_count = message_refs.len();
    if message_count == 0 {
        return CompactionEligibility::InvalidInput {
            failure: CompactionEligibilityFailure::ContextEmpty,
            message_count,
            ref_count,
        };
    }
    if ref_count != message_count {
        return CompactionEligibility::InvalidInput {
            failure: CompactionEligibilityFailure::MessageRefLengthMismatch,
            message_count,
            ref_count,
        };
    }

    let start_index = first_uncompacted_message_index(messages);
    let cut = find_cut_point(messages, start_index, params);
    if cut == 0 {
        return CompactionEligibility::NoEligibleMessages {
            message_count,
            keep_last_n: params.keep_last_n,
        };
    }

    if message_refs
        .get(cut.saturating_sub(1))
        .and_then(Option::as_ref)
        .is_none()
    {
        return CompactionEligibility::InvalidInput {
            failure: CompactionEligibilityFailure::CompactedUntilRefMissing,
            message_count,
            ref_count,
        };
    }

    if cut < messages.len() && message_refs.get(cut).and_then(Option::as_ref).is_none() {
        return CompactionEligibility::InvalidInput {
            failure: CompactionEligibilityFailure::FirstKeptRefMissing,
            message_count,
            ref_count,
        };
    }

    CompactionEligibility::Eligible
}

/// 执行压缩：cut point -> 摘要生成 -> 消息替换
pub async fn execute_compaction(
    messages: &[AgentMessage],
    message_refs: &[Option<MessageRef>],
    params: &CompactionParams,
    metadata: CompactionMetadata,
    bridge: &dyn LlmBridge,
    cancel: &CancellationToken,
) -> Result<Option<CompactionResult>, AgentError> {
    if message_refs.len() != messages.len() {
        return Err(AgentError::InvalidState(
            "compaction_message_ref_len_mismatch".to_string(),
        ));
    }
    let start_index = first_uncompacted_message_index(messages);
    let cut_index = find_cut_point(messages, start_index, params);
    if cut_index == 0 {
        return Ok(None); // 没有可压缩的消息
    }
    let compacted_until_ref = message_refs
        .get(cut_index.saturating_sub(1))
        .and_then(Clone::clone)
        .ok_or_else(|| AgentError::InvalidState("compaction_boundary_ref_missing".to_string()))?;
    let first_kept_ref = if cut_index < messages.len() {
        Some(
            message_refs
                .get(cut_index)
                .and_then(Clone::clone)
                .ok_or_else(|| {
                    AgentError::InvalidState("compaction_first_kept_ref_missing".to_string())
                })?,
        )
    } else {
        None
    };

    let previous_summary = existing_summary(messages);
    let summary = if let Some(ref custom) = params.custom_summary {
        custom.clone()
    } else {
        let messages_to_summarize = &messages[start_index..cut_index];
        let refs_to_summarize = &message_refs[start_index..cut_index];
        generate_summary(
            bridge,
            messages_to_summarize,
            refs_to_summarize,
            previous_summary.as_deref(),
            params.custom_prompt.as_deref(),
            cancel,
        )
        .await?
    };
    if summary.trim().is_empty() {
        return Err(AgentError::InvalidState("summary_empty".to_string()));
    }
    let used_custom_summary = params.custom_summary.is_some();

    let previously_compacted = previously_compacted_count(messages);
    let newly_compacted_messages = (cut_index - start_index) as u32;
    let total_compacted_messages = previously_compacted + newly_compacted_messages;

    // 构建压缩摘要消息
    let summary_message = AgentMessage::compaction_summary_with_boundary(
        summary,
        params.trigger_stats.input_tokens,
        total_compacted_messages,
        Some(compacted_until_ref.clone()),
    );

    // 替换消息：[CompactionSummary] + [kept_messages]
    let mut projected_messages = vec![summary_message.clone()];
    projected_messages.extend(messages[cut_index..].iter().cloned());
    let mut projected_refs = vec![None];
    projected_refs.extend(message_refs[cut_index..].iter().cloned());

    Ok(Some(CompactionResult {
        messages: projected_messages,
        message_refs: projected_refs,
        summary_message,
        compacted_until_ref,
        first_kept_ref,
        trigger_stats: params.trigger_stats.clone(),
        metadata,
        newly_compacted_messages,
        used_custom_summary,
    }))
}

pub fn should_execute_compaction(
    messages: &[AgentMessage],
    message_refs: &[Option<MessageRef>],
    params: &CompactionParams,
) -> bool {
    matches!(
        evaluate_compaction_eligibility(messages, message_refs, params),
        CompactionEligibility::Eligible
    )
}

/// 确定 cut point（按 token budget 保留尾部，并用 keep_last_n 作为最低保护）。
///
/// 规则：
/// - 从末尾按估算 token 反推可保留尾部
/// - 最少保留 keep_last_n 条消息
/// - Cut point 必须在 tool_call / tool_result 对边界上
/// - 如果第一条消息是 CompactionSummary，从它之后开始计数
///
/// 返回：cut_index，即 messages[start_index..cut_index] 将被压缩。
fn find_cut_point(
    messages: &[AgentMessage],
    start_index: usize,
    params: &CompactionParams,
) -> usize {
    let keep_last_n = params.keep_last_n as usize;
    if messages.len() <= keep_last_n {
        return 0;
    }

    let mut cut = token_budget_cut_point(messages, start_index, params)
        .unwrap_or_else(|| messages.len().saturating_sub(keep_last_n));
    if cut <= start_index {
        return 0; // 没有足够的非摘要消息可压缩
    }

    // 确保 cut point 不在 tool_call/tool_result 对中间
    // 规则：如果 cut point 处是 ToolResult，向前移动直到找到非 ToolResult 消息
    while cut < messages.len() && matches!(messages[cut], AgentMessage::ToolResult { .. }) {
        cut += 1;
    }

    // 确保 cut point 处不是 Assistant（它的 ToolResult 可能在后面）
    // 如果 Assistant 有 tool_calls，其 ToolResult 必须跟着
    if cut > 0
        && let AgentMessage::Assistant { tool_calls, .. } = &messages[cut - 1]
        && !tool_calls.is_empty()
    {
        // 前一条是有 tool_calls 的 Assistant，它的 results 可能在 cut 处
        // 需要把 Assistant 和它的 ToolResults 一起保留
        cut -= 1;
        // 继续向前找到这组 Assistant+ToolResults 的起始
        while cut > start_index && matches!(messages[cut], AgentMessage::ToolResult { .. }) {
            cut -= 1;
        }
    }

    if cut <= start_index {
        return 0;
    }

    cut
}

fn token_budget_cut_point(
    messages: &[AgentMessage],
    start_index: usize,
    params: &CompactionParams,
) -> Option<usize> {
    let target_tokens = params
        .trigger_stats
        .context_window
        .saturating_sub(params.reserve_tokens)
        .max(1);
    if params.trigger_stats.context_window == 0 {
        return None;
    }

    let keep_last_n = params.keep_last_n as usize;
    let min_tail_start = messages.len().saturating_sub(keep_last_n);
    let mut tail_tokens = 0_u64;
    let mut cut = messages.len();

    for idx in (start_index..messages.len()).rev() {
        let message_tokens = estimate_message_tokens(&messages[idx]);
        let must_keep = idx >= min_tail_start;
        if must_keep || tail_tokens.saturating_add(message_tokens) <= target_tokens {
            tail_tokens = tail_tokens.saturating_add(message_tokens);
            cut = idx;
        } else {
            break;
        }
    }

    if cut <= start_index { None } else { Some(cut) }
}

fn first_uncompacted_message_index(messages: &[AgentMessage]) -> usize {
    messages
        .iter()
        .position(|m| !matches!(m, AgentMessage::CompactionSummary { .. }))
        .unwrap_or(messages.len())
}

fn existing_summary(messages: &[AgentMessage]) -> Option<String> {
    messages.iter().find_map(|message| match message {
        AgentMessage::CompactionSummary { summary, .. } => Some(summary.clone()),
        _ => None,
    })
}

fn previously_compacted_count(messages: &[AgentMessage]) -> u32 {
    messages
        .iter()
        .find_map(|message| match message {
            AgentMessage::CompactionSummary {
                messages_compacted, ..
            } => Some(*messages_compacted),
            _ => None,
        })
        .unwrap_or(0)
}

fn build_summary_request_messages(
    messages_to_summarize: &[AgentMessage],
    _message_refs: &[Option<MessageRef>],
    previous_summary: Option<&str>,
) -> Vec<AgentMessage> {
    let instruction = if let Some(prev) = previous_summary {
        format!(
            "\
请基于以上新增对话历史更新已有摘要。

## 已有摘要

<summary>
{prev}
</summary>

## 输出要求

输出完整 markdown 摘要。不要把本说明当作对话历史内容。"
        )
    } else {
        "\
请基于以上对话历史生成交接摘要。

## 输出要求

输出完整 markdown 摘要。不要把本说明当作对话历史内容。"
            .to_string()
    };

    let mut messages = Vec::with_capacity(messages_to_summarize.len() + 1);
    messages.extend(messages_to_summarize.iter().cloned());
    messages.push(AgentMessage::User {
        content: vec![ContentPart::text(instruction)],
        timestamp: None,
    });
    messages
}

/// 调用 LLM 生成摘要
async fn generate_summary(
    bridge: &dyn LlmBridge,
    messages_to_summarize: &[AgentMessage],
    message_refs: &[Option<MessageRef>],
    previous_summary: Option<&str>,
    custom_prompt: Option<&str>,
    cancel: &CancellationToken,
) -> Result<String, AgentError> {
    let system_prompt = if previous_summary.is_some() {
        custom_prompt.unwrap_or(UPDATE_SUMMARIZATION_PROMPT)
    } else {
        custom_prompt.unwrap_or(SUMMARIZATION_SYSTEM_PROMPT)
    };

    let request = BridgeRequest {
        system_prompt: Some(system_prompt.to_string()),
        messages: build_summary_request_messages(
            messages_to_summarize,
            message_refs,
            previous_summary,
        ),
        tools: vec![], // 摘要生成不需要工具
    };

    let mut stream = bridge.stream_complete(request).await;
    let mut result_text = String::new();

    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        if cancel.is_cancelled() {
            return Err(AgentError::Cancelled);
        }
        match chunk {
            StreamChunk::TextDelta(text) => {
                result_text.push_str(&text);
            }
            StreamChunk::Done(resp) => {
                // 从最终响应提取完整文本
                if let AgentMessage::Assistant { content, .. } = &resp.message {
                    result_text = content
                        .iter()
                        .filter_map(|p| p.extract_text())
                        .collect::<Vec<_>>()
                        .join("");
                }
                break;
            }
            StreamChunk::Error(e) => {
                return Err(AgentError::Bridge(e));
            }
            _ => {}
        }
    }

    if result_text.trim().is_empty() {
        return Err(AgentError::InvalidState("summary_empty".to_string()));
    }

    Ok(result_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{BridgeResponse, LlmBridge};
    use crate::types::{ContentPart, TokenUsage, ToolCallInfo};
    use async_trait::async_trait;
    use std::pin::Pin;
    use tokio::sync::Mutex;

    struct RecordingBridge {
        requests: Mutex<Vec<BridgeRequest>>,
    }
    struct EmptySummaryBridge;

    impl RecordingBridge {
        fn new() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl LlmBridge for RecordingBridge {
        async fn stream_complete(
            &self,
            request: BridgeRequest,
        ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            self.requests.lock().await.push(request);
            Box::pin(tokio_stream::once(StreamChunk::Done(BridgeResponse {
                message: AgentMessage::Assistant {
                    content: vec![ContentPart::text("summary")],
                    tool_calls: vec![],
                    stop_reason: None,
                    error_message: None,
                    usage: Some(TokenUsage::default()),
                    timestamp: None,
                },
                raw_content: vec![ContentPart::text("summary")],
                usage: TokenUsage::default(),
            })))
        }
    }

    #[async_trait]
    impl LlmBridge for EmptySummaryBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
            Box::pin(tokio_stream::once(StreamChunk::Done(BridgeResponse {
                message: AgentMessage::Assistant {
                    content: vec![ContentPart::text("   ")],
                    tool_calls: vec![],
                    stop_reason: None,
                    error_message: None,
                    usage: Some(TokenUsage::default()),
                    timestamp: None,
                },
                raw_content: vec![ContentPart::text("   ")],
                usage: TokenUsage::default(),
            })))
        }
    }

    fn trigger_stats() -> crate::types::CompactionTriggerStats {
        crate::types::CompactionTriggerStats {
            input_tokens: 48_000,
            context_window: 64_000,
            reserve_tokens: 16_384,
        }
    }

    fn compaction_params(keep_last_n: u32, reserve_tokens: u64) -> CompactionParams {
        CompactionParams {
            keep_last_n,
            reserve_tokens,
            custom_summary: Some("merged summary".to_string()),
            custom_prompt: None,
            trigger_stats: trigger_stats(),
            metadata: default_auto_compaction_metadata(),
        }
    }

    fn message_refs(count: usize) -> Vec<Option<MessageRef>> {
        (0..count)
            .map(|index| {
                Some(MessageRef {
                    turn_id: format!("t-{index}"),
                    entry_index: 0,
                })
            })
            .collect()
    }

    #[test]
    fn eligibility_reports_true_no_eligible_messages() {
        let messages = vec![AgentMessage::user("u1"), AgentMessage::assistant("a1")];

        assert_eq!(
            evaluate_compaction_eligibility(
                &messages,
                &message_refs(messages.len()),
                &compaction_params(20, 16_384),
            ),
            CompactionEligibility::NoEligibleMessages {
                message_count: 2,
                keep_last_n: 20,
            }
        );
    }

    #[test]
    fn eligibility_rejects_empty_context() {
        assert_eq!(
            evaluate_compaction_eligibility(&[], &[], &compaction_params(1, 16_384)),
            CompactionEligibility::InvalidInput {
                failure: CompactionEligibilityFailure::ContextEmpty,
                message_count: 0,
                ref_count: 0,
            }
        );
    }

    #[test]
    fn eligibility_rejects_message_ref_length_mismatch() {
        let messages = vec![
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];

        assert_eq!(
            evaluate_compaction_eligibility(
                &messages,
                &message_refs(messages.len() - 1),
                &compaction_params(1, 16_384),
            ),
            CompactionEligibility::InvalidInput {
                failure: CompactionEligibilityFailure::MessageRefLengthMismatch,
                message_count: 3,
                ref_count: 2,
            }
        );
    }

    #[test]
    fn eligibility_rejects_missing_compacted_until_ref() {
        let messages = vec![
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];
        let mut refs = message_refs(messages.len());
        refs[1] = None;

        assert_eq!(
            evaluate_compaction_eligibility(&messages, &refs, &compaction_params(1, 16_384)),
            CompactionEligibility::InvalidInput {
                failure: CompactionEligibilityFailure::CompactedUntilRefMissing,
                message_count: 3,
                ref_count: 3,
            }
        );
    }

    #[test]
    fn eligibility_rejects_missing_first_kept_ref() {
        let messages = vec![
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];
        let mut refs = message_refs(messages.len());
        refs[2] = None;

        assert_eq!(
            evaluate_compaction_eligibility(&messages, &refs, &compaction_params(1, 16_384)),
            CompactionEligibility::InvalidInput {
                failure: CompactionEligibilityFailure::FirstKeptRefMissing,
                message_count: 3,
                ref_count: 3,
            }
        );
    }

    #[tokio::test]
    async fn execute_compaction_skips_existing_summary_when_building_new_summary() {
        let bridge = RecordingBridge::new();
        let messages = vec![
            AgentMessage::compaction_summary("old summary", 32_000, 3),
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];

        let result = execute_compaction(
            &messages,
            &message_refs(messages.len()),
            &compaction_params(1, 16_384),
            default_auto_compaction_metadata(),
            &bridge,
            &CancellationToken::new(),
        )
        .await
        .expect("compaction should succeed")
        .expect("compaction should produce result");

        assert_eq!(result.newly_compacted_messages, 2);
        assert!(matches!(
            result.messages.first(),
            Some(AgentMessage::CompactionSummary {
                messages_compacted: 5,
                ..
            })
        ));
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[1].first_text(), Some("u2"));
    }

    #[tokio::test]
    async fn summary_generation_sends_native_messages_to_bridge() {
        let bridge = RecordingBridge::new();
        let tool_call = ToolCallInfo {
            id: "tool-read-1".to_string(),
            call_id: Some("call-read-1".to_string()),
            name: "read_file".to_string(),
            arguments: serde_json::json!({ "path": "src/main.rs" }),
        };
        let messages = vec![
            AgentMessage::user("请读取文件"),
            AgentMessage::Assistant {
                content: vec![ContentPart::text("我来读取。")],
                tool_calls: vec![tool_call.clone()],
                stop_reason: None,
                error_message: None,
                usage: None,
                timestamp: None,
            },
            AgentMessage::tool_result_full(
                "tool-read-1",
                Some("call-read-1".to_string()),
                Some("read_file".to_string()),
                vec![
                    ContentPart::text("fn main() {}"),
                    ContentPart::Image {
                        mime_type: "image/png".to_string(),
                        data: "AAECAw==".to_string(),
                    },
                ],
                Some(serde_json::json!({ "bytes": 12 })),
                false,
            ),
            AgentMessage::user("继续"),
        ];

        execute_compaction(
            &messages,
            &message_refs(messages.len()),
            &CompactionParams {
                custom_summary: None,
                ..compaction_params(1, 16_384)
            },
            default_auto_compaction_metadata(),
            &bridge,
            &CancellationToken::new(),
        )
        .await
        .expect("compaction should succeed")
        .expect("compaction should produce result");

        let requests = bridge.requests.lock().await;
        let request = requests.first().expect("summary request should be sent");
        assert_eq!(request.messages.len(), 4);
        assert_eq!(request.messages[0], messages[0]);
        assert_eq!(request.messages[1], messages[1]);
        assert_eq!(request.messages[2], messages[2]);

        match &request.messages[1] {
            AgentMessage::Assistant { tool_calls, .. } => assert_eq!(tool_calls, &[tool_call]),
            other => panic!("expected assistant with tool calls, got {other:?}"),
        }
        match &request.messages[2] {
            AgentMessage::ToolResult {
                content, details, ..
            } => {
                assert!(matches!(content.get(1), Some(ContentPart::Image { .. })));
                assert_eq!(
                    details.as_ref().and_then(|d| d.get("bytes")),
                    Some(&serde_json::json!(12))
                );
            }
            other => panic!("expected native tool result, got {other:?}"),
        }

        let instruction = request.messages[3].first_text().expect("instruction");
        assert!(instruction.contains("生成交接摘要"));
        assert!(!instruction.contains("Lifecycle"));
        assert!(!instruction.contains("session/messages"));
        let system_prompt = request.system_prompt.as_deref().expect("system prompt");
        for expected in [
            "只能总结提供的消息",
            "不能继续对话",
            "当前目标",
            "当前进展",
            "关键决策",
            "约束",
            "文件、工具",
            "错误",
            "待办事项",
            "下一步",
        ] {
            assert!(
                system_prompt.contains(expected),
                "summary prompt should mention {expected}"
            );
        }
        assert!(
            !instruction.contains("[User]:"),
            "summary request should not serialize history into transcript text"
        );
    }

    #[tokio::test]
    async fn summary_update_keeps_previous_summary_as_instruction_not_flattened_history() {
        let bridge = RecordingBridge::new();
        let messages = vec![
            AgentMessage::compaction_summary("old summary", 32_000, 3),
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];

        execute_compaction(
            &messages,
            &message_refs(messages.len()),
            &CompactionParams {
                custom_summary: None,
                ..compaction_params(1, 16_384)
            },
            default_auto_compaction_metadata(),
            &bridge,
            &CancellationToken::new(),
        )
        .await
        .expect("compaction should succeed")
        .expect("compaction should produce result");

        let requests = bridge.requests.lock().await;
        let request = requests.first().expect("summary request should be sent");
        assert_eq!(
            request.system_prompt.as_deref(),
            Some(UPDATE_SUMMARIZATION_PROMPT)
        );
        assert_eq!(request.messages.len(), 3);
        assert_eq!(request.messages[0], messages[1]);
        assert_eq!(request.messages[1], messages[2]);
        let instruction = request.messages[2].first_text().expect("instruction");
        assert!(instruction.contains("<summary>\nold summary\n</summary>"));
        assert!(!instruction.contains("Lifecycle"));
        assert!(
            !request
                .messages
                .iter()
                .any(|message| matches!(message, AgentMessage::CompactionSummary { .. }))
        );
    }

    #[test]
    fn find_cut_point_respects_existing_summary_boundary() {
        let messages = vec![
            AgentMessage::compaction_summary("old summary", 32_000, 4),
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
            AgentMessage::assistant("a2"),
        ];

        let start_index = first_uncompacted_message_index(&messages);
        assert_eq!(
            find_cut_point(&messages, start_index, &compaction_params(2, 16_384)),
            3
        );
    }

    #[tokio::test]
    async fn execute_compaction_rejects_empty_summary() {
        let bridge = EmptySummaryBridge;
        let messages = vec![
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];

        let error = execute_compaction(
            &messages,
            &message_refs(messages.len()),
            &CompactionParams {
                custom_summary: None,
                ..compaction_params(1, 16_384)
            },
            default_auto_compaction_metadata(),
            &bridge,
            &CancellationToken::new(),
        )
        .await
        .expect_err("empty summary should be a compaction failure");

        assert!(error.to_string().contains("summary_empty"));
    }

    #[test]
    fn reserve_tokens_changes_cut_point() {
        let messages = vec![
            AgentMessage::user("短"),
            AgentMessage::assistant("前段回复 ".repeat(2_000)),
            AgentMessage::user("中间"),
            AgentMessage::assistant("近期回复 ".repeat(2_000)),
            AgentMessage::user("最新"),
        ];
        let start_index = first_uncompacted_message_index(&messages);
        let relaxed = find_cut_point(
            &messages,
            start_index,
            &CompactionParams {
                trigger_stats: crate::types::CompactionTriggerStats {
                    input_tokens: 4_000,
                    context_window: 4_000,
                    reserve_tokens: 500,
                },
                ..compaction_params(1, 500)
            },
        );
        let tight = find_cut_point(
            &messages,
            start_index,
            &CompactionParams {
                trigger_stats: crate::types::CompactionTriggerStats {
                    input_tokens: 4_000,
                    context_window: 4_000,
                    reserve_tokens: 3_000,
                },
                ..compaction_params(1, 3_000)
            },
        );

        assert!(tight >= relaxed);
    }
}
