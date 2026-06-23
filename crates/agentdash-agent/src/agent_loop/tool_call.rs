use std::sync::Arc;

use jsonschema::validator_for;
use tokio_util::sync::CancellationToken;

use crate::types::{
    AfterToolCallContext, AfterToolCallInput, AgentContext, AgentError, AgentEvent, AgentMessage,
    AgentToolResult, BeforeToolCallContext, BeforeToolCallInput, DynAgentTool, ToolApprovalOutcome,
    ToolApprovalRequest, ToolCallDecision, ToolCallInfo, ToolDefinition, ToolExecutionMode,
    ToolUpdateCallback,
};

use super::tool_result::{
    AgentToolResultCacheWrite, AgentToolResultInlineKind, approval_rejected_tool_result,
    bound_agent_tool_result_text, error_tool_result,
};
use super::{
    AgentEventSink, AgentLoopConfig, ToolResultCacheWrite, ToolResultRefContext, emit_event,
};

pub(super) async fn execute_tool_calls(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    match config.tool_execution {
        ToolExecutionMode::Sequential => {
            execute_tool_calls_sequential(
                context,
                tool_instances,
                assistant_message,
                tool_calls,
                config,
                emit,
                cancel,
            )
            .await
        }
        ToolExecutionMode::Parallel => {
            execute_tool_calls_parallel(
                context,
                tool_instances,
                assistant_message,
                tool_calls,
                config,
                emit,
                cancel,
            )
            .await
        }
    }
}

/// 顺序执行 — 对齐 Pi `executeToolCallsSequential` (agent-loop.ts:350-388)
async fn execute_tool_calls_sequential(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut results = Vec::new();

    for tc in tool_calls {
        emit_event(
            emit,
            AgentEvent::ToolExecutionStart {
                tool_call_id: tc.id.clone(),
                tool_name: tc.name.clone(),
                args: tc.arguments.clone(),
            },
        )
        .await;

        let preparation = prepare_tool_call(
            context,
            tool_instances,
            assistant_message,
            tc,
            config,
            cancel,
        )
        .await;

        match preparation {
            ToolCallPreparation::Immediate { result, is_error } => {
                let bounded = bound_tool_result_for_call(
                    tc,
                    &result,
                    AgentToolResultInlineKind::Final,
                    config,
                );
                results.push(emit_tool_call_outcome(tc, &bounded, is_error, emit).await);
            }
            ToolCallPreparation::AwaitApproval {
                tool,
                args,
                reason,
                details,
            } => {
                match await_tool_approval(tc, &args, &reason, details, config, emit, cancel).await {
                    ApprovalResolution::Approved => {
                        let executed =
                            execute_prepared_tool_call(tc, &tool, &args, config, cancel, emit)
                                .await;
                        let finalized = finalize_executed_tool_call(
                            context,
                            assistant_message,
                            tc,
                            &args,
                            executed,
                            config,
                            cancel,
                        )
                        .await;
                        let bounded = bound_tool_result_for_call(
                            tc,
                            &finalized.result,
                            AgentToolResultInlineKind::Final,
                            config,
                        );
                        results.push(
                            emit_tool_call_outcome(tc, &bounded, finalized.is_error, emit).await,
                        );
                    }
                    ApprovalResolution::Rejected { result } => {
                        let bounded = bound_tool_result_for_call(
                            tc,
                            &result,
                            AgentToolResultInlineKind::Final,
                            config,
                        );
                        results.push(emit_tool_result_message(tc, &bounded, emit).await);
                    }
                }
            }
            ToolCallPreparation::Prepared { tool, args } => {
                let executed =
                    execute_prepared_tool_call(tc, &tool, &args, config, cancel, emit).await;
                let finalized = finalize_executed_tool_call(
                    context,
                    assistant_message,
                    tc,
                    &args,
                    executed,
                    config,
                    cancel,
                )
                .await;
                let bounded = bound_tool_result_for_call(
                    tc,
                    &finalized.result,
                    AgentToolResultInlineKind::Final,
                    config,
                );
                results.push(emit_tool_call_outcome(tc, &bounded, finalized.is_error, emit).await);
            }
        }
    }

    Ok(results)
}

/// 并行执行 — 对齐 Pi `executeToolCallsParallel` (agent-loop.ts:390-438)
///
/// 顺序 prepare → 并发 execute → 顺序 finalize + emit
async fn execute_tool_calls_parallel(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tool_calls: &[ToolCallInfo],
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> Result<Vec<AgentMessage>, AgentError> {
    let mut results = Vec::new();

    struct PreparedEntry {
        tc: ToolCallInfo,
        tool: DynAgentTool,
        args: serde_json::Value,
    }

    let mut runnable: Vec<PreparedEntry> = Vec::new();

    for tc in tool_calls {
        emit_event(
            emit,
            AgentEvent::ToolExecutionStart {
                tool_call_id: tc.id.clone(),
                tool_name: tc.name.clone(),
                args: tc.arguments.clone(),
            },
        )
        .await;

        let preparation = prepare_tool_call(
            context,
            tool_instances,
            assistant_message,
            tc,
            config,
            cancel,
        )
        .await;

        match preparation {
            ToolCallPreparation::Immediate { result, is_error } => {
                let bounded = bound_tool_result_for_call(
                    tc,
                    &result,
                    AgentToolResultInlineKind::Final,
                    config,
                );
                results.push(emit_tool_call_outcome(tc, &bounded, is_error, emit).await);
            }
            ToolCallPreparation::AwaitApproval {
                tool,
                args,
                reason,
                details,
            } => {
                match await_tool_approval(tc, &args, &reason, details, config, emit, cancel).await {
                    ApprovalResolution::Approved => {
                        runnable.push(PreparedEntry {
                            tc: tc.clone(),
                            tool,
                            args,
                        });
                    }
                    ApprovalResolution::Rejected { result } => {
                        let bounded = bound_tool_result_for_call(
                            tc,
                            &result,
                            AgentToolResultInlineKind::Final,
                            config,
                        );
                        results.push(emit_tool_result_message(tc, &bounded, emit).await);
                    }
                }
            }
            ToolCallPreparation::Prepared { tool, args } => {
                runnable.push(PreparedEntry {
                    tc: tc.clone(),
                    tool,
                    args,
                });
            }
        }
    }

    // Phase 2: 并发 execute — 对齐 Pi: 每个工具获得独立 on_update 回调
    let handles: Vec<_> = runnable
        .iter()
        .map(|entry| {
            let tool = entry.tool.clone();
            let tc_id = entry.tc.id.clone();
            let args = entry.args.clone();
            let cancel = cancel.clone();
            let on_update = Some(build_on_update(
                &entry.tc,
                emit,
                config.tool_result_ref_context.clone(),
            ));
            tokio::spawn(async move {
                execute_prepared_tool_call_inner(&tc_id, &tool, &args, cancel, on_update).await
            })
        })
        .collect();

    let executed_results: Vec<ExecutedOutcome> = {
        let mut out = Vec::with_capacity(handles.len());
        for handle in handles {
            out.push(handle.await.unwrap_or(ExecutedOutcome {
                result: error_tool_result("工具执行 task panic"),
                is_error: true,
            }));
        }
        out
    };

    // Phase 3: 顺序 finalize + emit
    for (entry, executed) in runnable.iter().zip(executed_results) {
        let finalized = finalize_executed_tool_call(
            context,
            assistant_message,
            &entry.tc,
            &entry.args,
            executed,
            config,
            cancel,
        )
        .await;
        let bounded = bound_tool_result_for_call(
            &entry.tc,
            &finalized.result,
            AgentToolResultInlineKind::Final,
            config,
        );
        results.push(emit_tool_call_outcome(&entry.tc, &bounded, finalized.is_error, emit).await);
    }

    Ok(results)
}

// ─── 三阶段工具执行 ─────────────────────────────────────────

enum ToolCallPreparation {
    /// 立即返回结果（工具不存在、参数无效、被 beforeToolCall 阻止）
    Immediate {
        result: AgentToolResult,
        is_error: bool,
    },
    /// 等待用户审批后再决定是否执行
    AwaitApproval {
        tool: DynAgentTool,
        args: serde_json::Value,
        reason: String,
        details: Option<serde_json::Value>,
    },
    /// 准备就绪，可以执行
    Prepared {
        tool: DynAgentTool,
        args: serde_json::Value,
    },
}

struct ExecutedOutcome {
    result: AgentToolResult,
    is_error: bool,
}

/// Phase 1: prepare — 对齐 Pi `prepareToolCall` (agent-loop.ts:458-507)
///
/// 从 tool_instances 查找工具 → validate → delegate 钩子 → 返回 Prepared / Immediate
async fn prepare_tool_call(
    context: &AgentContext,
    tool_instances: &[DynAgentTool],
    assistant_message: &AgentMessage,
    tc: &ToolCallInfo,
    config: &AgentLoopConfig,
    cancel: &CancellationToken,
) -> ToolCallPreparation {
    let current_tools = current_tool_instances(tool_instances, config);
    let tool = current_tools.iter().find(|t| t.name() == tc.name);
    let tool = match tool {
        Some(t) => t.clone(),
        None => {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(format!("Tool {} not found", tc.name)),
                is_error: true,
            };
        }
    };

    let mut args = match validate_tool_call_arguments(&tool, tc) {
        Ok(args) => args,
        Err(error) => {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(error),
                is_error: true,
            };
        }
    };

    if let Some(delegate) = config.runtime_delegate.as_ref() {
        let mut hook_context = context.clone();
        apply_tool_definitions(&mut hook_context, &current_tools);
        let input = BeforeToolCallInput {
            assistant_message: assistant_message.clone(),
            tool_call: tc.clone(),
            args: args.clone(),
            context: hook_context,
        };
        let decision = match delegate.before_tool_call(input, cancel.clone()).await {
            Ok(decision) => decision,
            Err(error) => {
                return ToolCallPreparation::Immediate {
                    result: error_tool_result(format!(
                        "runtime delegate before_tool_call 失败: {error}"
                    )),
                    is_error: true,
                };
            }
        };

        match decision {
            ToolCallDecision::Allow => {}
            ToolCallDecision::Deny { reason } => {
                return ToolCallPreparation::Immediate {
                    result: error_tool_result(reason),
                    is_error: true,
                };
            }
            ToolCallDecision::Ask {
                reason,
                args: approval_args,
                details,
            } => {
                if config.await_tool_approval.is_none() {
                    return ToolCallPreparation::Immediate {
                        result: error_tool_result(
                            "runtime delegate 请求审批，但当前 Agent 未配置审批等待能力",
                        ),
                        is_error: true,
                    };
                }
                let args = match approval_args {
                    Some(rewritten) => match validate_tool_arguments(&tool, &tc.name, &rewritten) {
                        Ok(validated) => validated,
                        Err(error) => {
                            return ToolCallPreparation::Immediate {
                                result: error_tool_result(error),
                                is_error: true,
                            };
                        }
                    },
                    None => args,
                };
                return ToolCallPreparation::AwaitApproval {
                    tool,
                    args,
                    reason,
                    details,
                };
            }
            ToolCallDecision::Rewrite {
                args: rewritten, ..
            } => match validate_tool_arguments(&tool, &tc.name, &rewritten) {
                Ok(validated) => args = validated,
                Err(error) => {
                    return ToolCallPreparation::Immediate {
                        result: error_tool_result(error),
                        is_error: true,
                    };
                }
            },
        }
    }

    if let Some(ref hook) = config.before_tool_call {
        let mut hook_context = context.clone();
        apply_tool_definitions(&mut hook_context, &current_tools);
        let ctx = BeforeToolCallContext {
            assistant_message,
            tool_call: tc,
            args: &args,
            context: &hook_context,
        };
        if let Some(before_result) = hook(ctx, cancel.clone()).await
            && before_result.block
        {
            return ToolCallPreparation::Immediate {
                result: error_tool_result(
                    before_result
                        .reason
                        .unwrap_or_else(|| "Tool execution was blocked".to_string()),
                ),
                is_error: true,
            };
        }
    }

    ToolCallPreparation::Prepared { tool, args }
}

fn current_tool_instances(
    fallback_tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
) -> Vec<DynAgentTool> {
    config
        .get_tools
        .as_ref()
        .map(|get_tools| get_tools())
        .unwrap_or_else(|| fallback_tool_instances.to_vec())
}

pub(super) fn refresh_context_tools(
    context: &mut AgentContext,
    fallback_tool_instances: &[DynAgentTool],
    config: &AgentLoopConfig,
) -> Vec<DynAgentTool> {
    let tools = current_tool_instances(fallback_tool_instances, config);
    apply_tool_definitions(context, &tools);
    tools
}

fn apply_tool_definitions(context: &mut AgentContext, tools: &[DynAgentTool]) {
    context.tools = tools
        .iter()
        .map(|tool| ToolDefinition::from_tool(tool.as_ref()))
        .collect();
}

/// Phase 2: execute — 对齐 Pi `executePreparedToolCall` (agent-loop.ts:509-544)
async fn execute_prepared_tool_call(
    tc: &ToolCallInfo,
    tool: &DynAgentTool,
    args: &serde_json::Value,
    config: &AgentLoopConfig,
    cancel: &CancellationToken,
    emit: &AgentEventSink,
) -> ExecutedOutcome {
    let on_update = build_on_update(tc, emit, config.tool_result_ref_context.clone());
    execute_prepared_tool_call_inner(&tc.id, tool, args, cancel.clone(), Some(on_update)).await
}

/// 构建 `on_update` 回调 — 对齐 Pi `executePreparedToolCall` 内联闭包
fn build_on_update(
    tc: &ToolCallInfo,
    emit: &AgentEventSink,
    ref_context: Option<ToolResultRefContext>,
) -> ToolUpdateCallback {
    let emit = emit.clone();
    let tc_id = tc.id.clone();
    let tc_name = tc.name.clone();
    let tc_args = tc.arguments.clone();
    Arc::new(move |partial_result: AgentToolResult| {
        let emit = emit.clone();
        let bounded_partial = bound_tool_result_for_call_with_context(
            &tc_id,
            &tc_name,
            &partial_result,
            AgentToolResultInlineKind::Update,
            ref_context.as_ref(),
        );
        let event = AgentEvent::ToolExecutionUpdate {
            tool_call_id: tc_id.clone(),
            tool_name: tc_name.clone(),
            args: tc_args.clone(),
            partial_result: serde_json::to_value(&bounded_partial).unwrap_or_default(),
        };
        tokio::spawn(async move {
            emit(event).await;
        });
    })
}

async fn execute_prepared_tool_call_inner(
    tool_call_id: &str,
    tool: &DynAgentTool,
    args: &serde_json::Value,
    cancel: CancellationToken,
    on_update: Option<ToolUpdateCallback>,
) -> ExecutedOutcome {
    match tool
        .execute(tool_call_id, args.clone(), cancel, on_update)
        .await
    {
        Ok(result) => {
            let is_error = result.is_error;
            ExecutedOutcome { result, is_error }
        }
        Err(e) => ExecutedOutcome {
            result: error_tool_result(format!("{e}")),
            is_error: true,
        },
    }
}

/// Phase 3: finalize — 对齐 Pi `finalizeExecutedToolCall` (agent-loop.ts:546-580)
///
/// 调用 afterToolCall 钩子，允许覆盖结果。
async fn finalize_executed_tool_call(
    context: &AgentContext,
    assistant_message: &AgentMessage,
    tc: &ToolCallInfo,
    args: &serde_json::Value,
    executed: ExecutedOutcome,
    config: &AgentLoopConfig,
    cancel: &CancellationToken,
) -> ExecutedOutcome {
    let mut result = executed.result;
    let mut is_error = executed.is_error;

    if let Some(delegate) = config.runtime_delegate.as_ref() {
        let input = AfterToolCallInput {
            assistant_message: assistant_message.clone(),
            tool_call: tc.clone(),
            args: args.clone(),
            result: result.clone(),
            is_error,
            context: context.clone(),
        };

        match delegate.after_tool_call(input, cancel.clone()).await {
            Ok(effects) => {
                if let Some(content) = effects.content {
                    result.content = content;
                }
                if let Some(details) = effects.details {
                    result.details = Some(details);
                }
                if let Some(err) = effects.is_error {
                    is_error = err;
                }
            }
            Err(error) => {
                return ExecutedOutcome {
                    result: error_tool_result(format!(
                        "runtime delegate after_tool_call 失败: {error}"
                    )),
                    is_error: true,
                };
            }
        }
    }

    if let Some(ref hook) = config.after_tool_call {
        let ctx = AfterToolCallContext {
            assistant_message,
            tool_call: tc,
            args,
            result: &result,
            is_error,
            context,
        };
        if let Some(after_result) = hook(ctx, cancel.clone()).await {
            if let Some(content) = after_result.content {
                result.content = content;
            }
            if let Some(details) = after_result.details {
                result.details = Some(details);
            }
            if let Some(err) = after_result.is_error {
                is_error = err;
            }
        }
    }

    result.is_error = is_error;
    ExecutedOutcome { result, is_error }
}

// ─── 辅助函数 ───────────────────────────────────────────────

fn validate_tool_call_arguments(
    tool: &DynAgentTool,
    tc: &ToolCallInfo,
) -> Result<serde_json::Value, String> {
    validate_tool_arguments(tool, &tc.name, &tc.arguments)
}

fn validate_tool_arguments(
    tool: &DynAgentTool,
    tool_name: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let schema = tool.parameters_schema();
    let validator = validator_for(&schema)
        .map_err(|error| format!("Tool {} schema is invalid: {error}", tool_name))?;
    let errors = validator
        .iter_errors(args)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(args.clone())
    } else {
        Err(format!(
            "Tool {} arguments are invalid: {}",
            tool_name,
            errors.join("; ")
        ))
    }
}

/// 发出工具执行结果事件并构建 ToolResult 消息
/// 对齐 Pi `emitToolCallOutcome` (agent-loop.ts:589-616)
async fn emit_tool_call_outcome(
    tc: &ToolCallInfo,
    result: &AgentToolResult,
    is_error: bool,
    emit: &AgentEventSink,
) -> AgentMessage {
    emit_event(
        emit,
        AgentEvent::ToolExecutionEnd {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            result: serde_json::to_value(result).unwrap_or_else(|_| {
                serde_json::json!({
                    "content": result.content,
                    "is_error": is_error,
                    "details": result.details,
                })
            }),
            is_error,
        },
    )
    .await;

    emit_tool_result_message(tc, result, emit).await
}

async fn emit_tool_result_message(
    tc: &ToolCallInfo,
    result: &AgentToolResult,
    emit: &AgentEventSink,
) -> AgentMessage {
    let tool_result_msg = AgentMessage::tool_result_full(
        &tc.id,
        tc.call_id.clone(),
        Some(tc.name.clone()),
        result.content.clone(),
        result.details.clone(),
        result.is_error,
    );

    emit_event(
        emit,
        AgentEvent::MessageStart {
            message: tool_result_msg.clone(),
        },
    )
    .await;
    emit_event(
        emit,
        AgentEvent::MessageEnd {
            message: tool_result_msg.clone(),
        },
    )
    .await;

    tool_result_msg
}

fn bound_tool_result_for_call(
    tc: &ToolCallInfo,
    result: &AgentToolResult,
    inline_kind: AgentToolResultInlineKind,
    config: &AgentLoopConfig,
) -> AgentToolResult {
    bound_tool_result_for_call_with_context(
        &tc.id,
        &tc.name,
        result,
        inline_kind,
        config.tool_result_ref_context.as_ref(),
    )
}

fn bound_tool_result_for_call_with_context(
    tool_call_id: &str,
    tool_name: &str,
    result: &AgentToolResult,
    inline_kind: AgentToolResultInlineKind,
    ref_context: Option<&ToolResultRefContext>,
) -> AgentToolResult {
    let readable_ref = ref_context
        .map(|context| {
            context
                .readable_ids
                .tool_result_ref(&context.raw_turn_id, tool_call_id, tool_name)
        })
        .unwrap_or_else(|| {
            let fallback_registry = super::ReadableIdRegistry::default();
            fallback_registry.tool_result_ref("turn", tool_call_id, tool_name)
        });
    let item_id = readable_ref.item_id.as_str();
    let lifecycle_path = readable_ref.lifecycle_path.as_str();
    let bounded =
        bound_agent_tool_result_text(result, item_id, lifecycle_path, inline_kind, |write| {
            record_agent_tool_result_cache_write(
                ref_context,
                Some(&readable_ref),
                tool_name,
                write,
            );
        });
    let should_attach_readable_ref = ref_context.is_some()
        || bounded.details.as_ref().is_some_and(|details| {
            details.get("truncation").is_some() || details.get("lifecycle_path").is_some()
        });
    if should_attach_readable_ref {
        attach_readable_ref_details(bounded, &readable_ref, tool_name)
    } else {
        bounded
    }
}

fn attach_readable_ref_details(
    mut result: AgentToolResult,
    readable_ref: &super::ReadableToolResultRef,
    tool_name: &str,
) -> AgentToolResult {
    let mut details = match result.details.take() {
        Some(serde_json::Value::Object(object)) => object,
        Some(value) => {
            let mut object = serde_json::Map::new();
            object.insert("original_details".to_string(), value);
            object
        }
        None => serde_json::Map::new(),
    };
    details.insert(
        "readable_ref".to_string(),
        serde_json::json!({
            "item_id": readable_ref.item_id,
            "turn_alias": readable_ref.turn_alias,
            "body_alias": readable_ref.body_alias,
            "body_kind": readable_ref.body_kind.as_str(),
            "lifecycle_path": readable_ref.lifecycle_path,
        }),
    );
    details.insert(
        "raw_trace".to_string(),
        serde_json::json!({
            "raw_turn_id": readable_ref.raw_turn_id,
            "raw_tool_call_id": readable_ref.raw_tool_call_id,
            "tool_name": tool_name,
        }),
    );
    result.details = Some(serde_json::Value::Object(details));
    result
}

fn record_agent_tool_result_cache_write(
    ref_context: Option<&ToolResultRefContext>,
    readable_ref: Option<&super::ReadableToolResultRef>,
    tool_name: &str,
    write: AgentToolResultCacheWrite<'_>,
) {
    if let (Some(context), Some(readable_ref)) = (ref_context, readable_ref)
        && let Some(writer) = context.cache_writer.as_ref()
    {
        writer(ToolResultCacheWrite {
            session_id: context.session_id.clone(),
            item_id: write.item_id.to_string(),
            lifecycle_path: write.lifecycle_path.to_string(),
            turn_alias: readable_ref.turn_alias.clone(),
            body_alias: readable_ref.body_alias.clone(),
            body_kind: readable_ref.body_kind.as_str().to_string(),
            raw_turn_id: readable_ref.raw_turn_id.clone(),
            raw_tool_call_id: readable_ref.raw_tool_call_id.clone(),
            tool_name: tool_name.to_string(),
            text: write.text.to_string(),
            original_bytes: write.original_bytes,
        });
        return;
    }

    tracing::debug!(
        item_id = write.item_id,
        lifecycle_path = write.lifecycle_path,
        original_bytes = write.original_bytes,
        cache_text_bytes = write.text.len(),
        "agent tool result cache write requested"
    );
}

enum ApprovalResolution {
    Approved,
    Rejected { result: AgentToolResult },
}

async fn await_tool_approval(
    tc: &ToolCallInfo,
    args: &serde_json::Value,
    reason: &str,
    details: Option<serde_json::Value>,
    config: &AgentLoopConfig,
    emit: &AgentEventSink,
    cancel: &CancellationToken,
) -> ApprovalResolution {
    emit_event(
        emit,
        AgentEvent::ToolExecutionPendingApproval {
            tool_call_id: tc.id.clone(),
            tool_name: tc.name.clone(),
            args: args.clone(),
            reason: reason.to_string(),
            details: details.clone(),
        },
    )
    .await;

    let Some(await_approval) = config.await_tool_approval.as_ref() else {
        return ApprovalResolution::Rejected {
            result: approval_rejected_tool_result(Some(
                "当前 Agent 未配置审批等待能力".to_string(),
            )),
        };
    };

    match await_approval(
        ToolApprovalRequest {
            tool_call: tc.clone(),
            args: args.clone(),
            reason: reason.to_string(),
            details,
        },
        cancel.clone(),
    )
    .await
    {
        ToolApprovalOutcome::Approved => {
            emit_event(
                emit,
                AgentEvent::ToolExecutionApprovalResolved {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    args: args.clone(),
                    approved: true,
                    reason: None,
                },
            )
            .await;
            ApprovalResolution::Approved
        }
        ToolApprovalOutcome::Rejected { reason } => {
            emit_event(
                emit,
                AgentEvent::ToolExecutionApprovalResolved {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    args: args.clone(),
                    approved: false,
                    reason: reason.clone(),
                },
            )
            .await;
            ApprovalResolution::Rejected {
                result: approval_rejected_tool_result(reason),
            }
        }
    }
}
