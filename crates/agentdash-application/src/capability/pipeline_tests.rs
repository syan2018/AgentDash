//! Capability pipeline 集成测试
//!
//! 覆盖 agent_node + phase_node 两种 step 激活场景在 domain → application 层的
//! 完整数据流：
//!
//! - step ToolCapabilityDirective → workflow baseline + directive 运算 → effective key 集合
//! - SetDelta::compute → 前后差异
//! - CapabilityResolver::resolve(workflow_tool_directives) → 实际 CapabilityState
//! - build_capability_delta_markdown → 供 agent 直接消费的通知文本

#![cfg(test)]

use std::collections::BTreeSet;

use agentdash_domain::session_binding::SessionOwnerCtx;
use agentdash_domain::workflow::ToolCapabilityDirective;
use agentdash_spi::SetDelta;
use agentdash_spi::ToolCluster;
use uuid::Uuid;

use crate::capability::{
    AgentMcpServerEntry, CapabilityResolver, CapabilityResolverInput, ContextContributionSource,
    ContextContributions, McpCandidates, ToolContribution, build_capability_delta_markdown,
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
        server: agentdash_spi::SessionMcpServer {
            name: name.to_string(),
            transport: agentdash_spi::McpTransportConfig::Http {
                url: url.to_string(),
                headers: vec![],
            },
            uses_relay: false,
        },
    }
}

fn state_has_mcp_url(output: &crate::capability::CapabilityResolverOutput, needle: &str) -> bool {
    output.tool.mcp_servers.iter().any(|server| {
        matches!(
            &server.transport,
            agentdash_spi::McpTransportConfig::Http { url, .. } if url.contains(needle)
        )
    })
}

/// agent_node 场景：workflow contract 直接给出一串 Add/Remove → Resolver 产出 session 工具集。
#[test]
fn agent_node_step_directives_produce_expected_session_tools() {
    let directives = vec![
        ToolCapabilityDirective::add_simple("workflow_management"),
        ToolCapabilityDirective::add_simple("mcp:code_analyzer"),
        ToolCapabilityDirective::remove_simple("collaboration"),
    ];

    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        contributions: vec![ContextContributions {
            source: ContextContributionSource::Workflow,
            tool: Some(ToolContribution {
                directives: directives.clone(),
                has_active_workflow: true,
            }),
            companion: None,
        }],
        mcp_candidates: McpCandidates {
            presets: Default::default(),
            agent_servers: vec![mcp_entry("code_analyzer", "http://external:8080/mcp")],
        },
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    assert!(output.has(ToolCluster::Read));
    assert!(output.has(ToolCluster::Write));
    assert!(output.has(ToolCluster::Execute));
    // collaboration 已被 Remove
    assert!(!output.has(ToolCluster::Collaboration));

    // workflow_management → 平台 Workflow MCP
    assert!(
        state_has_mcp_url(&output, "/mcp/workflow/"),
        "应注入 WorkflowMcpServer"
    );

    // mcp:code_analyzer → 自定义 MCP 出现在统一 CapabilityState 中
    assert!(
        output
            .tool
            .mcp_servers
            .iter()
            .any(|server| server.name == "code_analyzer")
    );
}

/// phase_node 场景：directive 直接表达增删，resolver 产出一致的 effective 结果。
#[test]
fn phase_node_transition_produces_delta_markdown_and_updated_mcp() {
    let directives = vec![
        ToolCapabilityDirective::add_simple("workflow_management"),
        ToolCapabilityDirective::add_simple("mcp:external_analyzer"),
        ToolCapabilityDirective::remove_simple("canvas"),
    ];

    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        contributions: vec![ContextContributions {
            source: ContextContributionSource::Workflow,
            tool: Some(ToolContribution {
                directives: directives.clone(),
                has_active_workflow: true,
            }),
            companion: None,
        }],
        mcp_candidates: McpCandidates {
            presets: Default::default(),
            agent_servers: vec![mcp_entry("external_analyzer", "http://external:9000/mcp")],
        },
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    assert!(!output.has(ToolCluster::Canvas));
    assert!(output.has(ToolCluster::Read));
    assert!(state_has_mcp_url(&output, "/mcp/workflow/"));
    assert!(
        output
            .tool
            .mcp_servers
            .iter()
            .any(|server| server.name == "external_analyzer")
    );

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
        .tool
        .capabilities
        .iter()
        .map(|c| c.key().to_string())
        .collect();
    let delta = SetDelta::compute(&baseline_set, &effective_set);

    let md = build_capability_delta_markdown("implement", &delta, &effective_set, None);
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
        contributions: Vec::new(),
        mcp_candidates: McpCandidates::default(),
    };
    let output = CapabilityResolver::resolve(&input, &platform());

    // baseline 自带的能力
    assert!(output.has(ToolCluster::Read));
    assert!(output.has(ToolCluster::Write));
    assert!(output.has(ToolCluster::Canvas));

    let effective_set: BTreeSet<String> = output
        .tool
        .capabilities
        .iter()
        .map(|c| c.key().to_string())
        .collect();
    let delta = SetDelta::compute(&effective_set, &effective_set);
    assert!(delta.is_empty());
}

/// Remove 不存在的 key 静默失败；mcp:* 未注册时 resolver 跳过。
#[test]
fn phase_node_invalid_directives_are_tolerated() {
    let directives = vec![
        ToolCapabilityDirective::remove_simple("never_existed"),
        ToolCapabilityDirective::add_simple("mcp:missing_server"),
    ];

    let input = CapabilityResolverInput {
        owner_ctx: SessionOwnerCtx::Project {
            project_id: Uuid::new_v4(),
        },
        contributions: vec![ContextContributions {
            source: ContextContributionSource::Workflow,
            tool: Some(ToolContribution {
                directives: directives.clone(),
                has_active_workflow: true,
            }),
            companion: None,
        }],
        mcp_candidates: McpCandidates::default(),
    };
    let output = CapabilityResolver::resolve(&input, &platform());
    assert!(
        !output
            .tool
            .mcp_servers
            .iter()
            .any(|server| server.name == "missing_server")
    );
    assert!(
        !output
            .tool
            .capabilities
            .iter()
            .any(|cap| cap.key() == "mcp:missing_server")
    );
}
