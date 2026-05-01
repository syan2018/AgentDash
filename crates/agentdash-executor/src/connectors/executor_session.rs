use std::{collections::HashMap, sync::Arc};

use agentdash_protocol::{
    BackboneEnvelope, BackboneEvent, PlatformEvent, SourceInfo, TraceInfo,
};
use agentdash_spi::{
    ConnectorError, ConnectorType, ExecutionContext, ExecutionStream, PromptPayload,
    workspace_path_from_context,
};
use executors::{
    approvals::NoopExecutorApprovalService,
    env::{ExecutionEnv, RepoContext},
    executors::{CancellationToken, CodingAgent, StandardCodingAgentExecutor as _},
    logs::utils::patch::extract_normalized_entry_from_patch,
};
use futures::StreamExt;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::io::ReaderStream;
use workspace_utils::{log_msg::LogMsg, msg_store::MsgStore};

use crate::adapters::normalized_to_backbone::NormalizedToBackboneConverter;

fn connector_type_label(connector_type: ConnectorType) -> &'static str {
    match connector_type {
        ConnectorType::LocalExecutor => "local_executor",
        ConnectorType::RemoteAcpBackend => "remote_acp_backend",
    }
}

/// 复用 executors crate 子进程桥接链路（spawn / normalize / ACP 转换）。
pub(crate) async fn spawn_executor_session(
    connector_id: &'static str,
    connector_type: ConnectorType,
    cancel_by_session: Arc<Mutex<HashMap<String, CancellationToken>>>,
    mut agent: CodingAgent,
    session_id: &str,
    follow_up_session_id: Option<&str>,
    prompt: &PromptPayload,
    context: ExecutionContext,
) -> Result<ExecutionStream, ConnectorError> {
    let user_text = prompt.to_fallback_text();
    // 过渡阶段沿用预渲染 system prompt，后续可替换为 Bundle 原生渲染。
    #[allow(deprecated)]
    let prompt_text = if let Some(ref sys_prompt) = context.turn.assembled_system_prompt {
        format!("{sys_prompt}\n\n{user_text}")
    } else {
        user_text
    };
    let prompt_text = prompt_text.trim().to_string();
    if prompt_text.is_empty() {
        return Err(ConnectorError::InvalidConfig(
            "prompt payload 解析后为空".to_string(),
        ));
    }

    agent.use_approvals(Arc::new(NoopExecutorApprovalService));

    let repo_root = workspace_path_from_context(&context)?;
    let repo_context = RepoContext::new(repo_root, vec![".".to_string()]);
    let mut env = ExecutionEnv::new(
        repo_context,
        false,
        "请在提交前完成 pnpm lint/type-check/test 等自检".to_string(),
    );
    env.merge(&context.session.environment_variables);

    let follow_up_session_id = follow_up_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut spawned = if let Some(follow_up_session_id) = follow_up_session_id {
        agent
            .spawn_follow_up(
                &context.session.working_directory,
                &prompt_text,
                follow_up_session_id,
                None,
                &env,
            )
            .await
            .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?
    } else {
        agent
            .spawn(&context.session.working_directory, &prompt_text, &env)
            .await
            .map_err(|e| ConnectorError::SpawnFailed(e.to_string()))?
    };

    if let Some(cancel) = spawned.cancel.clone() {
        cancel_by_session
            .lock()
            .await
            .insert(session_id.to_string(), cancel);
    }

    let msg_store = Arc::new(MsgStore::new());
    agent.normalize_logs(msg_store.clone(), &context.session.working_directory);

    if let Some(stdout) = spawned.child.inner().stdout.take() {
        let ms = msg_store.clone();
        tokio::spawn(async move {
            let mut stream = ReaderStream::new(stdout);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(bytes) => ms.push_stdout(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(e) => {
                        ms.push_stdout(format!("stdout 读取失败: {e}"));
                        break;
                    }
                }
            }
        });
    }

    if let Some(stderr) = spawned.child.inner().stderr.take() {
        let ms = msg_store.clone();
        tokio::spawn(async move {
            let mut stream = ReaderStream::new(stderr);
            while let Some(item) = stream.next().await {
                match item {
                    Ok(bytes) => ms.push_stdout(String::from_utf8_lossy(&bytes).into_owned()),
                    Err(e) => {
                        ms.push_stdout(format!("stderr 读取失败: {e}"));
                        break;
                    }
                }
            }
        });
    }

    let ms = msg_store.clone();
    let cancel_map = cancel_by_session.clone();
    let session_id_owned = session_id.to_string();
    let session_id_for_wait = session_id_owned.clone();
    tokio::spawn(async move {
        let _ = spawned.child.wait().await;
        ms.push_finished();
        cancel_map.lock().await.remove(&session_id_for_wait);
    });

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<BackboneEnvelope, ConnectorError>>(256);
    let source = SourceInfo {
        connector_id: connector_id.to_string(),
        connector_type: connector_type_label(connector_type).to_string(),
        executor_id: Some(context.session.executor_config.executor.to_string()),
    };
    let turn_id = context.session.turn_id.clone();
    let mut converter = NormalizedToBackboneConverter::new(
        session_id.to_string(),
        source.clone(),
        turn_id.clone(),
    );

    tokio::spawn(async move {
        let mut stream = msg_store.history_plus_stream();
        let mut last_executor_session_id: Option<String> = None;
        while let Some(next) = stream.next().await {
            match next {
                Ok(LogMsg::JsonPatch(patch)) => {
                    if let Some((idx, entry)) = extract_normalized_entry_from_patch(&patch) {
                        for envelope in converter.apply(idx, entry) {
                            if tx.send(Ok(envelope)).await.is_err() {
                                return;
                            }
                        }
                    }
                }
                Ok(LogMsg::SessionId(executor_session_id)) => {
                    if last_executor_session_id.as_deref() == Some(executor_session_id.as_str()) {
                        continue;
                    }
                    last_executor_session_id = Some(executor_session_id.clone());
                    let envelope = BackboneEnvelope::new(
                        BackboneEvent::Platform(PlatformEvent::ExecutorSessionBound {
                            executor_session_id,
                        }),
                        &session_id_owned,
                        source.clone(),
                    )
                    .with_trace(TraceInfo {
                        turn_id: Some(turn_id.clone()),
                        entry_index: None,
                    });
                    if tx.send(Ok(envelope)).await.is_err() {
                        return;
                    }
                }
                Ok(LogMsg::Finished) => break,
                Ok(_) => {}
                Err(e) => {
                    let _ = tx
                        .send(Err(ConnectorError::Runtime(format!("日志流错误: {e}"))))
                        .await;
                    break;
                }
            }
        }
    });

    Ok(Box::pin(ReceiverStream::new(rx)))
}
