use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;

use crate::ToolShellOutputPayload;

/// 串行 Shell 流式输出路由表。
///
/// 当 ShellExecTool 发起执行时注册一个 call_id → 通道，
/// WebSocket handler 收到 `EventToolShellOutput` 后通过此表转发到对应通道。
/// ShellExecTool 侧消费通道并通过 `ToolUpdateCallback` 向上汇报。
#[derive(Debug, Default)]
pub struct ShellOutputRegistry {
    sinks: RwLock<HashMap<String, mpsc::UnboundedSender<ToolShellOutputPayload>>>,
}

impl ShellOutputRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sinks: RwLock::new(HashMap::new()),
        })
    }

    pub fn register(&self, call_id: &str, tx: mpsc::UnboundedSender<ToolShellOutputPayload>) {
        self.sinks.write().unwrap().insert(call_id.to_string(), tx);
    }

    pub fn unregister(&self, call_id: &str) {
        self.sinks.write().unwrap().remove(call_id);
    }

    /// 将 `EventToolShellOutput` 的 payload 路由到对应的消费方。
    /// 返回 true 表示成功投递，false 表示无匹配 sink。
    pub fn route(&self, payload: &ToolShellOutputPayload) -> bool {
        let sinks = self.sinks.read().unwrap();
        if let Some(tx) = sinks.get(&payload.call_id) {
            tx.send(payload.clone()).is_ok()
        } else {
            false
        }
    }
}
