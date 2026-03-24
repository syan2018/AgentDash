use agentdash_plugin_api::AgentDashPlugin;

/// 开源版默认“无认证”插件骨架。
///
/// 当前仅用于声明开源版默认不启用认证，后续如需真正的开放认证策略，
/// 再决定是否实现显式的 `AuthProvider`。
pub struct NoAuthPlugin;

impl AgentDashPlugin for NoAuthPlugin {
    fn name(&self) -> &str {
        "builtin.no_auth"
    }
}

/// 开源版默认连接器目录插件骨架。
///
/// 当前连接器实际仍由宿主直接构建；该插件用于为 first-party plugin 目录预留位置，
/// 后续可以逐步把内置连接器迁移到真正的插件装配模型。
pub struct ConnectorCatalogPlugin;

impl AgentDashPlugin for ConnectorCatalogPlugin {
    fn name(&self) -> &str {
        "builtin.connector_catalog"
    }
}

pub fn builtin_plugins() -> Vec<Box<dyn AgentDashPlugin>> {
    vec![Box::new(NoAuthPlugin), Box::new(ConnectorCatalogPlugin)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_builtin_plugin_skeletons() {
        let names = builtin_plugins()
            .into_iter()
            .map(|plugin| plugin.name().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            vec![
                "builtin.no_auth".to_string(),
                "builtin.connector_catalog".to_string()
            ]
        );
    }
}
