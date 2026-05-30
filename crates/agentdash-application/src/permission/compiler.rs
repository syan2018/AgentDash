//! Permission Grant Compiler — 将 approved grant 翻译为 RuntimeCapabilityTransition。

use agentdash_domain::permission::PermissionGrant;
use agentdash_domain::workflow::ToolCapabilityDirective;
use agentdash_spi::{
    CapabilityArtifactSource, CapabilityDeclarationRecord, CapabilityDimensionKey,
    RuntimeCapabilityTransition,
};
use agentdash_spi::session_persistence::{
    CAPABILITY_DIMENSION_TOOL, DECLARATION_TYPE_CAPABILITY_DIRECTIVE,
};

/// 将 approved PermissionGrant 编译为可应用的 RuntimeCapabilityTransition。
pub struct PermissionGrantCompiler;

impl PermissionGrantCompiler {
    /// 从 grant 的 requested_paths 生成 Add directives。
    pub fn compile(grant: &PermissionGrant) -> RuntimeCapabilityTransition {
        let declarations: Vec<CapabilityDeclarationRecord> = grant
            .requested_paths
            .iter()
            .map(|path| CapabilityDeclarationRecord {
                dimension: CapabilityDimensionKey::new(CAPABILITY_DIMENSION_TOOL),
                declaration_type: DECLARATION_TYPE_CAPABILITY_DIRECTIVE.to_string(),
                source: CapabilityArtifactSource::permission_grant(),
                payload: serde_json::to_value(ToolCapabilityDirective::Add(path.clone()))
                    .unwrap_or_default(),
            })
            .collect();

        RuntimeCapabilityTransition {
            declarations,
            effects: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::permission::GrantScope;
    use agentdash_domain::workflow::ToolCapabilityPath;
    use uuid::Uuid;

    #[test]
    fn compile_produces_add_directives() {
        let grant = PermissionGrant::new(
            Uuid::new_v4(),
            "session-1",
            vec![
                ToolCapabilityPath::parse("story_management").unwrap(),
                ToolCapabilityPath::parse("task_management::start_task").unwrap(),
            ],
            "test",
            GrantScope::Session,
            None,
        );

        let transition = PermissionGrantCompiler::compile(&grant);
        assert_eq!(transition.declarations.len(), 2);
        assert_eq!(transition.declarations[0].dimension.as_str(), "tool");
        assert_eq!(
            transition.declarations[0].declaration_type,
            "capability_directive"
        );

        let directive: ToolCapabilityDirective =
            serde_json::from_value(transition.declarations[0].payload.clone()).unwrap();
        match directive {
            ToolCapabilityDirective::Add(path) => {
                assert_eq!(path.capability, "story_management");
            }
            _ => panic!("expected Add directive"),
        }
    }
}
