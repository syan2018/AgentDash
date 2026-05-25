use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{CapabilityConfig, InputPortDefinition, OutputPortDefinition};

/// Lifecycle node 类型：Agent Node 创建独立 session，Phase Node 在前一个 session 内切换 contract
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleNodeType {
    /// 创建独立 agent session 执行工作
    #[default]
    AgentNode,
    /// 不创建新 session，在前一个 session 内切换 workflow contract
    PhaseNode,
}
/// Lifecycle edge 类别：控制流 vs 数据流。
///
/// - `Flow`：无数据语义的顺序约束（前驱完成即激活后继）。
/// - `Artifact`：端口级数据依赖；自动蕴含 Flow 约束（B 消费 A.port → B dep A）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEdgeKind {
    Flow,
    Artifact,
}

fn default_edge_kind() -> LifecycleEdgeKind {
    // 既有持久化数据无 kind 字段时统一视为 artifact（历史边全部带 port）
    LifecycleEdgeKind::Artifact
}

/// Lifecycle DAG 边——控制流 + 数据流的统一承载。
///
/// `kind = Flow` 时 `from_port` / `to_port` 必须为 `None`；
/// `kind = Artifact` 时两者必须为 `Some`。
/// node 级别依赖通过 `node_deps_from_edges()` 从 flow/artifact 两类边统一计算。
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
pub struct LifecycleEdge {
    #[serde(default = "default_edge_kind")]
    pub kind: LifecycleEdgeKind,
    pub from_node: String,
    pub to_node: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_port: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_port: Option<String>,
}

impl LifecycleEdge {
    /// 构造控制流边：仅表达顺序约束，无 port。
    pub fn flow(from_node: impl Into<String>, to_node: impl Into<String>) -> Self {
        Self {
            kind: LifecycleEdgeKind::Flow,
            from_node: from_node.into(),
            to_node: to_node.into(),
            from_port: None,
            to_port: None,
        }
    }

    /// 构造 artifact 边：端口级数据依赖；隐含 flow 约束。
    pub fn artifact(
        from_node: impl Into<String>,
        from_port: impl Into<String>,
        to_node: impl Into<String>,
        to_port: impl Into<String>,
    ) -> Self {
        Self {
            kind: LifecycleEdgeKind::Artifact,
            from_node: from_node.into(),
            to_node: to_node.into(),
            from_port: Some(from_port.into()),
            to_port: Some(to_port.into()),
        }
    }

    pub fn is_flow(&self) -> bool {
        matches!(self.kind, LifecycleEdgeKind::Flow)
    }

    pub fn is_artifact(&self) -> bool {
        matches!(self.kind, LifecycleEdgeKind::Artifact)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
pub struct LifecycleStepDefinition {
    pub key: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_key: Option<String>,
    #[serde(default)]
    pub node_type: LifecycleNodeType,
    /// Step 级产出约束：该节点必须交付的 artifacts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<OutputPortDefinition>,
    /// Step 级消费声明：该节点从前驱接收的 artifacts
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_ports: Vec<InputPortDefinition>,
    /// Step 级顶层能力配置，应用顺序在 workflow contract 配置之后。
    #[serde(default, skip_serializing_if = "CapabilityConfig::is_empty")]
    pub capability_config: CapabilityConfig,
}

impl LifecycleStepDefinition {
    /// 返回修剪后的 workflow_key（去空白、过滤空串）。
    pub fn effective_workflow_key(&self) -> Option<&str> {
        self.workflow_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}
