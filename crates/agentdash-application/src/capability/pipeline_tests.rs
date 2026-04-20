//! Capability pipeline 集成测试
//!
//! 覆盖 agent_node + phase_node 两种 step 激活场景在 domain → application 层的
//! 完整数据流：
//!
//! - step CapabilityDirective → workflow baseline + directive 运算 → effective key 集合
//! - CapabilityDelta::compute → 前后差异
//! - CapabilityResolver::resolve(workflow_capabilities override) → 实际 FlowCapabilities
//!   + platform MCP configs + 自定义 mcp:* 注入
//! - build_capability_delta_markdown → 供 agent 直接消费的通知文本
//!
//! 不构造 SessionHub / Connector 真身（那部分由 connector 单测覆盖），
//! 本测试守护的是"静态→动态"切换时 pure function 链条的一致性。

#![cfg(test)]

use std::collections::BTreeSet;

use agentdash_domain::session_binding::SessionOwnerType;
use agentdash_domain::workflow::{CapabilityDirective, compute_effective_capabilities};
use agentdash_spi::hooks::CapabilityDelta;
use agentdash_spi::ToolCluster;
use uuid::Uuid;

use crate::capability::{
    AgentMcpServerEntry, CapabilityResolver, CapabilityResolverInput, SessionWorkflowContext,
    build_capability_delta_markdown,
};
use crate::platform_config::PlatformConfig;

fn platform() -> PlatformConfig {
    PlatformConfig {
        mcp_base_url: Some("http://localhost:3001".to_string()),
    }
}

fn mcp_entry(name: &str, url: &str) -> AgentMcpServerEntry {
    AgentMcpServerEntry {
        name: name.to_string(),
        server: agent_client_protocol::McpServer::Http(
            agent_client_protocol::McpServerHttp::new(name, url),
        ),
    }
}

/// agent_node 场景：workflow baseline + step Add/Remove → CapabilityResolver 产出新 session 工具集。
#[test]
fn agent_node_step_directives_produce_expected_session_tools() {
    // baseline: workflow 级别声明了 file_system + collaboration
    let baseline = vec!["file_system".to_string(), "collaboration".to_string()];

    // 新 step 追加 workflow_management 和一个 mcp:* 外部能力，移除 collaboration
    let directives = vec![
        CapabilityDirective::Add("workflow_management".to_string()),
        CapabilityDirective::Add("mcp:code_analyzer".to_string()),
        CapabilityDirective::Remove("collaboration".to_string()),
    ];

    let effective = compute_effective_capabilities(&baseline, &directives);
    let effective_set: BTreeSet<String> = effective.iter().cloned().collect();

    assert!(effective_set.contains("file_system"));
    assert!(effective_set.contains("workflow_management"));
    assert!(effective_set.contains("mcp:code_analyzer"));
    assert!(!effective_set.contains("collaboration"));

    // Orchestrator 在 create_agent_node_session 中会将 effective set 当作 workflow_capabilities
    // 传给 CapabilityResolver（全量替换）
    let input = CapabilityResolverInput {
        owner_type: SessionOwnerType::Project,
        project_id: Uuid::new_v4(),
        story_id: None,
        task_id: None,
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capabilities: Some(effective.clone()),
        },
        agent_mcp_servers: vec![mcp_entry("code_analyzer", "http://external:8080/mcp")],
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    // file_system → Read + Write + Execute
    assert!(output.flow_capabilities.has(ToolCluster::Read));
    assert!(output.flow_capabilities.has(ToolCluster::Write));
    assert!(output.flow_capabilities.has(ToolCluster::Execute));
    // collaboration 已被 Remove
    assert!(!output.flow_capabilities.has(ToolCluster::Collaboration));

    // workflow_management → 平台 Workflow MCP
    assert!(
        output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/workflow/")),
        "应注入 WorkflowMcpServer"
    );

    // mcp:code_analyzer → 自定义 MCP 出现在 custom_mcp_servers 中
    assert_eq!(output.custom_mcp_servers.len(), 1);
}

/// phase_node 场景：同一 session 内从 baseline 切换到新 effective，
/// delta / 结构化 Markdown / resolver 结果一起校验。
#[test]
fn phase_node_transition_produces_delta_markdown_and_updated_mcp() {
    let baseline = vec![
        "file_system".to_string(),
        "canvas".to_string(),
        "collaboration".to_string(),
    ];

    let directives = vec![
        CapabilityDirective::Add("workflow_management".to_string()),
        CapabilityDirective::Add("mcp:external_analyzer".to_string()),
        CapabilityDirective::Remove("canvas".to_string()),
    ];

    let effective = compute_effective_capabilities(&baseline, &directives);
    let effective_set: BTreeSet<String> = effective.iter().cloned().collect();
    let baseline_set: BTreeSet<String> = baseline.iter().cloned().collect();

    let delta = CapabilityDelta::compute(&baseline_set, &effective_set);
    assert_eq!(
        delta.added.iter().collect::<BTreeSet<_>>(),
        [
            &"mcp:external_analyzer".to_string(),
            &"workflow_management".to_string()
        ]
        .into_iter()
        .collect::<BTreeSet<_>>()
    );
    assert_eq!(delta.removed, vec!["canvas".to_string()]);

    // Resolver 用新的 effective 重建 MCP 注入集合（replace-set 语义输入侧）
    let input = CapabilityResolverInput {
        owner_type: SessionOwnerType::Project,
        project_id: Uuid::new_v4(),
        story_id: None,
        task_id: None,
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capabilities: Some(effective.clone()),
        },
        agent_mcp_servers: vec![mcp_entry(
            "external_analyzer",
            "http://external:9000/mcp",
        )],
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    // Canvas cluster 已经不在 effective，下次 agent 请求 tools 时看不到 Canvas 工具
    assert!(!output.flow_capabilities.has(ToolCluster::Canvas));
    // file_system 保留
    assert!(output.flow_capabilities.has(ToolCluster::Read));
    // 新增 Workflow 管理 MCP
    assert!(
        output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/workflow/")),
    );
    // 新增自定义 MCP
    assert_eq!(output.custom_mcp_servers.len(), 1);

    // Agent 会在 steering 队列收到的结构化 Markdown
    let md = build_capability_delta_markdown("implement", &delta, &effective_set);
    assert!(md.contains("## Capability Update — Step Transition: implement"));
    assert!(md.contains("### Added Capabilities"));
    assert!(md.contains("**workflow_management**"));
    assert!(md.contains("**mcp:external_analyzer**"));
    assert!(md.contains("### Removed Capabilities"));
    assert!(md.contains("**canvas**"));
    assert!(md.contains("（不再可用）"));
    // 对应 MCP 描述与 McpInjectionConfig::to_context_content 的口径一致
    assert!(md.contains("Workflow / Lifecycle 定义的查看、创建与编辑"));
}

/// step 未声明 capabilities 时完全继承 baseline，不产生 delta。
#[test]
fn phase_node_without_directives_inherits_baseline_and_emits_no_delta() {
    let baseline = vec!["file_system".to_string(), "workflow".to_string()];
    let effective = compute_effective_capabilities(&baseline, &[]);

    assert_eq!(effective, baseline);

    let baseline_set: BTreeSet<String> = baseline.iter().cloned().collect();
    let effective_set: BTreeSet<String> = effective.iter().cloned().collect();

    let delta = CapabilityDelta::compute(&baseline_set, &effective_set);
    assert!(delta.is_empty());
    // delta 为空时 advance_node 不会推送通知
}

/// Remove 不存在的 key 静默失败；mcp:* 未注册时 resolver 跳过。
#[test]
fn phase_node_invalid_directives_are_tolerated() {
    let baseline = vec!["file_system".to_string()];
    let directives = vec![
        CapabilityDirective::Remove("never_existed".to_string()),
        CapabilityDirective::Add("mcp:missing_server".to_string()),
    ];

    let effective = compute_effective_capabilities(&baseline, &directives);
    // Remove 不存在：noop
    // Add mcp:missing：key 进入 effective 但 resolver 会在注入阶段跳过
    assert!(effective.contains(&"mcp:missing_server".to_string()));

    let input = CapabilityResolverInput {
        owner_type: SessionOwnerType::Project,
        project_id: Uuid::new_v4(),
        story_id: None,
        task_id: None,
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capabilities: Some(effective.clone()),
        },
        agent_mcp_servers: vec![], // 故意不注册
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());
    // 未注册 server 的 mcp:* key 不会出现在 custom_mcp_servers
    assert!(output.custom_mcp_servers.is_empty());
    assert!(
        !output
            .effective_capabilities
            .iter()
            .any(|cap| cap.key() == "mcp:missing_server")
    );
}
