//! Capability pipeline 集成测试
//!
//! 覆盖 agent_node + phase_node 两种 step 激活场景在 domain → application 层的
//! 完整数据流：
//!
//! - step CapabilityDirective → workflow baseline + directive 运算 → effective key 集合
//! - CapabilityDelta::compute → 前后差异
//! - CapabilityResolver::resolve(workflow_capability_directives) → 实际 FlowCapabilities
//!   + platform MCP configs + 自定义 mcp:* 注入
//! - build_capability_delta_markdown → 供 agent 直接消费的通知文本

#![cfg(test)]

use std::collections::BTreeSet;

use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::workflow::CapabilityDirective;
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

/// agent_node 场景：workflow contract 直接给出一串 Add/Remove → Resolver 产出 session 工具集。
#[test]
fn agent_node_step_directives_produce_expected_session_tools() {
    let directives = vec![
        CapabilityDirective::add_simple("workflow_management"),
        CapabilityDirective::add_simple("mcp:code_analyzer"),
        CapabilityDirective::remove_simple("collaboration"),
    ];

    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capability_directives: Some(directives.clone()),
        },
        agent_mcp_servers: vec![mcp_entry("code_analyzer", "http://external:8080/mcp")],
        available_presets: Default::default(),
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    // file_read/write/shell_execute 由 auto_granted baseline 提供
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

/// phase_node 场景：directive 直接表达增删，resolver 产出一致的 effective 结果。
#[test]
fn phase_node_transition_produces_delta_markdown_and_updated_mcp() {
    let directives = vec![
        CapabilityDirective::add_simple("workflow_management"),
        CapabilityDirective::add_simple("mcp:external_analyzer"),
        CapabilityDirective::remove_simple("canvas"),
    ];

    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capability_directives: Some(directives.clone()),
        },
        agent_mcp_servers: vec![mcp_entry("external_analyzer", "http://external:9000/mcp")],
        available_presets: Default::default(),
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    assert!(!output.flow_capabilities.has(ToolCluster::Canvas));
    assert!(output.flow_capabilities.has(ToolCluster::Read));
    assert!(
        output
            .platform_mcp_configs
            .iter()
            .any(|c| c.endpoint_url().contains("/mcp/workflow/")),
    );
    assert_eq!(output.custom_mcp_servers.len(), 1);

    // 模拟 baseline → effective 差异，验证 delta markdown 渲染
    let baseline_set: BTreeSet<String> = [
        "file_read",
        "file_write",
        "shell_execute",
        "canvas",
        "collaboration",
        "relay_management",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let effective_set: BTreeSet<String> = output
        .effective_capabilities
        .iter()
        .map(|c| c.key().to_string())
        .collect();
    let delta = CapabilityDelta::compute(&baseline_set, &effective_set);

    let md = build_capability_delta_markdown("implement", &delta, &effective_set);
    assert!(md.contains("## Capability Update — Step Transition: implement"));
    assert!(md.contains("### Added Capabilities"));
    assert!(md.contains("**workflow_management**"));
    assert!(md.contains("**mcp:external_analyzer**"));
    assert!(md.contains("### Removed Capabilities"));
    assert!(md.contains("**canvas**"));
    assert!(md.contains("（不再可用）"));
    assert!(md.contains("Workflow / Lifecycle 定义的查看、创建与编辑"));
}

/// 无 directive 时 resolver 完全依赖 auto_granted baseline。
#[test]
fn phase_node_without_directives_inherits_baseline_and_emits_no_delta() {
    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext::NONE,
        agent_mcp_servers: vec![],
        available_presets: Default::default(),
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    // baseline 自带的能力
    assert!(output.flow_capabilities.has(ToolCluster::Read));
    assert!(output.flow_capabilities.has(ToolCluster::Write));
    assert!(output.flow_capabilities.has(ToolCluster::Canvas));

    let effective_set: BTreeSet<String> = output
        .effective_capabilities
        .iter()
        .map(|c| c.key().to_string())
        .collect();
    let delta = CapabilityDelta::compute(&effective_set, &effective_set);
    assert!(delta.is_empty());
}

/// Remove 不存在的 key 静默失败；mcp:* 未注册时 resolver 跳过。
#[test]
fn phase_node_invalid_directives_are_tolerated() {
    let directives = vec![
        CapabilityDirective::remove_simple("never_existed"),
        CapabilityDirective::add_simple("mcp:missing_server"),
    ];

    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        agent_declared_capabilities: None,
        workflow_ctx: SessionWorkflowContext {
            has_active_workflow: true,
            workflow_capability_directives: Some(directives.clone()),
        },
        agent_mcp_servers: vec![],
        available_presets: Default::default(),
        companion_slice_mode: None,
    };
    let output = CapabilityResolver::resolve(&input, &platform());
    assert!(output.custom_mcp_servers.is_empty());
    assert!(
        !output
            .effective_capabilities
            .iter()
            .any(|cap| cap.key() == "mcp:missing_server")
    );
}
