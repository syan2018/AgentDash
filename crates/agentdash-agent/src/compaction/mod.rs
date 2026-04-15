//! 上下文压缩引擎
//!
//! 纯函数 + LLM 调用编排。策略决策由 AgentRuntimeDelegate 负责，
//! 本模块只负责执行：cut point 检测、摘要生成、消息替换。

use tokio_util::sync::CancellationToken;

use crate::bridge::{BridgeRequest, LlmBridge, StreamChunk};
use crate::types::{AgentError, AgentMessage, CompactionParams, CompactionResult};

/// 默认摘要 system prompt
const SUMMARIZATION_SYSTEM_PROMPT: &str = "\
你是一个会话摘要助手。请为以下 AI 编程助手与用户的对话历史生成一份简洁的结构化摘要。

摘要必须包含：
- 已完成的主要工作
- 做出的关键决策和原因
- 当前状态和待处理事项
- 重要的技术发现（文件路径、函数名等具体信息）

要求：
- 保留所有具体的文件路径、函数名、变量名
- 保留所有未完成的工作和待办事项
- 使用结构化的 markdown 格式
- 简洁但不遗漏关键信息";

/// 迭代式摘要更新 prompt（与前次摘要合并）
const UPDATE_SUMMARIZATION_PROMPT: &str = "\
你是一个会话摘要助手。你将收到一份已有摘要和一段新的对话历史。

请将新对话中的信息合并到已有摘要中：
- 保留已有摘要中仍然相关的信息
- 添加新对话中的新信息
- 更新已变更的状态
- 删除已过时的信息

输出更新后的完整摘要（markdown 格式）。";

/// 执行压缩：cut point -> 摘要生成 -> 消息替换
pub async fn execute_compaction(
    messages: &[AgentMessage],
    params: &CompactionParams,
    bridge: &dyn LlmBridge,
    cancel: &CancellationToken,
) -> Result<Option<CompactionResult>, AgentError> {
    let start_index = first_uncompacted_message_index(messages);
    let cut_index = find_cut_point(messages, start_index, params.keep_last_n as usize);
    if cut_index == 0 {
        return Ok(None); // 没有可压缩的消息
    }

    let previous_summary = existing_summary(messages);
    let summary = if let Some(ref custom) = params.custom_summary {
        custom.clone()
    } else {
        let messages_to_summarize = &messages[start_index..cut_index];
        generate_summary(
            bridge,
            messages_to_summarize,
            previous_summary.as_deref(),
            params.custom_prompt.as_deref(),
            cancel,
        )
        .await?
    };
    let used_custom_summary = params.custom_summary.is_some();

    let previously_compacted = previously_compacted_count(messages);
    let newly_compacted_messages = (cut_index - start_index) as u32;
    let total_compacted_messages = previously_compacted + newly_compacted_messages;

    // 构建压缩摘要消息
    let summary_message = AgentMessage::compaction_summary(
        summary,
        params.trigger_stats.input_tokens,
        total_compacted_messages,
    );

    // 替换消息：[CompactionSummary] + [kept_messages]
    let mut projected_messages = vec![summary_message.clone()];
    projected_messages.extend(messages[cut_index..].iter().cloned());

    Ok(Some(CompactionResult {
        messages: projected_messages,
        summary_message,
        trigger_stats: params.trigger_stats.clone(),
        newly_compacted_messages,
        used_custom_summary,
    }))
}

/// 确定 cut point（保留最后 keep_last_n 条消息）。
///
/// 规则：
/// - 从末尾向前数 keep_last_n 条消息
/// - Cut point 必须在 tool_call / tool_result 对边界上
/// - 如果第一条消息是 CompactionSummary，从它之后开始计数
///
/// 返回：cut_index，即 messages[start_index..cut_index] 将被压缩。
fn find_cut_point(messages: &[AgentMessage], start_index: usize, keep_last_n: usize) -> usize {
    if messages.len() <= keep_last_n {
        return 0;
    }

    // 计算初始 cut point
    let mut cut = messages.len().saturating_sub(keep_last_n);
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
    if cut > 0 {
        if let AgentMessage::Assistant { tool_calls, .. } = &messages[cut - 1] {
            if !tool_calls.is_empty() {
                // 前一条是有 tool_calls 的 Assistant，它的 results 可能在 cut 处
                // 需要把 Assistant 和它的 ToolResults 一起保留
                cut -= 1;
                // 继续向前找到这组 Assistant+ToolResults 的起始
                while cut > start_index && matches!(messages[cut], AgentMessage::ToolResult { .. })
                {
                    cut -= 1;
                }
            }
        }
    }

    if cut <= start_index {
        return 0;
    }

    cut
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

/// 将消息序列化为文本，用于发给摘要 LLM
fn serialize_messages_for_summary(messages: &[AgentMessage]) -> String {
    let mut text = String::new();
    for msg in messages {
        match msg {
            AgentMessage::User { content, .. } => {
                text.push_str("[User]: ");
                for part in content {
                    if let Some(t) = part.extract_text() {
                        text.push_str(t);
                    }
                }
                text.push('\n');
            }
            AgentMessage::Assistant {
                content,
                tool_calls,
                ..
            } => {
                text.push_str("[Assistant]: ");
                for part in content {
                    if let Some(t) = part.extract_text() {
                        text.push_str(t);
                    }
                }
                for tc in tool_calls {
                    text.push_str(&format!(
                        "\n  [Tool Call: {}({})]",
                        tc.name,
                        truncate_str(
                            &serde_json::to_string(&tc.arguments).unwrap_or_default(),
                            500
                        )
                    ));
                }
                text.push('\n');
            }
            AgentMessage::ToolResult {
                tool_name,
                content,
                is_error,
                ..
            } => {
                let name = tool_name.as_deref().unwrap_or("unknown");
                let prefix = if *is_error {
                    "[Tool Error"
                } else {
                    "[Tool Result"
                };
                text.push_str(&format!("{prefix}: {name}]: "));
                for part in content {
                    if let Some(t) = part.extract_text() {
                        text.push_str(truncate_str(t, 2000));
                    }
                }
                text.push('\n');
            }
            AgentMessage::CompactionSummary { summary, .. } => {
                text.push_str("[Previous Summary]: ");
                text.push_str(summary);
                text.push('\n');
            }
        }
    }
    text
}

fn truncate_str(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // 安全截断（不破坏 UTF-8 边界）
        let mut end = max_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

/// 调用 LLM 生成摘要
async fn generate_summary(
    bridge: &dyn LlmBridge,
    messages_to_summarize: &[AgentMessage],
    previous_summary: Option<&str>,
    custom_prompt: Option<&str>,
    cancel: &CancellationToken,
) -> Result<String, AgentError> {
    let conversation_text = serialize_messages_for_summary(messages_to_summarize);

    let (system_prompt, user_content) = if let Some(prev) = previous_summary {
        let system = custom_prompt.unwrap_or(UPDATE_SUMMARIZATION_PROMPT);
        let user = format!("## 已有摘要\n\n{prev}\n\n## 新增对话历史\n\n{conversation_text}");
        (system, user)
    } else {
        let system = custom_prompt.unwrap_or(SUMMARIZATION_SYSTEM_PROMPT);
        let user = format!("## 对话历史\n\n{conversation_text}");
        (system, user)
    };

    let request = BridgeRequest {
        system_prompt: Some(system_prompt.to_string()),
        messages: vec![AgentMessage::user(user_content)],
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

    if result_text.is_empty() {
        result_text = "[摘要生成失败 - 对话历史已被截断]".to_string();
    }

    Ok(result_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{BridgeResponse, LlmBridge};
    use crate::types::{ContentPart, TokenUsage};
    use async_trait::async_trait;
    use std::pin::Pin;

    struct RecordingBridge;

    #[async_trait]
    impl LlmBridge for RecordingBridge {
        async fn stream_complete(
            &self,
            _request: BridgeRequest,
        ) -> Pin<Box<dyn futures::Stream<Item = StreamChunk> + Send>> {
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

    fn trigger_stats() -> crate::types::CompactionTriggerStats {
        crate::types::CompactionTriggerStats {
            input_tokens: 48_000,
            context_window: 64_000,
            reserve_tokens: 16_384,
        }
    }

    #[tokio::test]
    async fn execute_compaction_skips_existing_summary_when_building_new_summary() {
        let bridge = RecordingBridge;
        let messages = vec![
            AgentMessage::compaction_summary("old summary", 32_000, 3),
            AgentMessage::user("u1"),
            AgentMessage::assistant("a1"),
            AgentMessage::user("u2"),
        ];

        let result = execute_compaction(
            &messages,
            &CompactionParams {
                keep_last_n: 1,
                reserve_tokens: 16_384,
                custom_summary: Some("merged summary".to_string()),
                custom_prompt: None,
                trigger_stats: trigger_stats(),
            },
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
        assert_eq!(find_cut_point(&messages, start_index, 2), 3);
    }
}
