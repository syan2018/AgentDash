use agentdash_domain::context_source::{ContextSlot, ContextSourceKind, ContextSourceRef};
use agentdash_spi::{
    ContextFragment, InjectionError, MergeStrategy, ResolveSourcesOutput, ResolveSourcesRequest,
    SourceResolver,
};

/// 来源解析器注册表 — 按 ContextSourceKind 注册解析器
///
/// 内置解析器在创建时自动注册，外部可通过 `register` 扩展新的来源类型。
pub struct SourceResolverRegistry {
    resolvers: std::collections::HashMap<ContextSourceKind, Box<dyn SourceResolver>>,
}

impl SourceResolverRegistry {
    /// 创建包含内置解析器的注册表
    pub fn with_builtins() -> Self {
        let mut registry = Self {
            resolvers: std::collections::HashMap::new(),
        };
        registry.register(ContextSourceKind::ManualText, Box::new(ManualTextResolver));
        registry
    }

    /// 注册新的来源解析器
    pub fn register(&mut self, kind: ContextSourceKind, resolver: Box<dyn SourceResolver>) {
        self.resolvers.insert(kind, resolver);
    }

    /// 查找指定 kind 的解析器
    pub fn get(&self, kind: &ContextSourceKind) -> Option<&dyn SourceResolver> {
        self.resolvers.get(kind).map(|r| r.as_ref())
    }

    pub fn supported_kinds(&self) -> Vec<&ContextSourceKind> {
        self.resolvers.keys().collect()
    }
}

impl Default for SourceResolverRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

pub fn resolve_declared_sources(
    request: ResolveSourcesRequest<'_>,
) -> Result<ResolveSourcesOutput, InjectionError> {
    resolve_declared_sources_with_registry(request, &SourceResolverRegistry::with_builtins())
}

/// 使用指定注册表解析声明式上下文来源
pub fn resolve_declared_sources_with_registry(
    request: ResolveSourcesRequest<'_>,
    registry: &SourceResolverRegistry,
) -> Result<ResolveSourcesOutput, InjectionError> {
    let mut indexed_sources = request.sources.iter().enumerate().collect::<Vec<_>>();
    indexed_sources.sort_by(|(left_index, left), (right_index, right)| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left_index.cmp(right_index))
    });

    let mut fragments = Vec::new();
    let mut warnings = Vec::new();

    for (position, source) in indexed_sources
        .into_iter()
        .map(|(_, source)| source)
        .enumerate()
    {
        let order = request.base_order + position as i32;
        let resolver = registry.get(&source.kind);

        let resolved = match resolver {
            Some(r) => r.resolve(source, order),
            None => {
                let msg = format!(
                    "source `{}` 的类型 {:?} 暂无已注册的解析器",
                    display_source_label(source),
                    source.kind
                );
                if source.required {
                    return Err(InjectionError::MissingWorkspace(msg));
                }
                warnings.push(msg);
                continue;
            }
        };

        match resolved {
            Ok(fragment) => fragments.push(fragment),
            Err(err) if source.required => return Err(err),
            Err(err) => warnings.push(format!(
                "source `{}` 已跳过: {err}",
                display_source_label(source)
            )),
        }
    }

    Ok(ResolveSourcesOutput {
        fragments,
        warnings,
    })
}

struct ManualTextResolver;

impl SourceResolver for ManualTextResolver {
    fn resolve(
        &self,
        source: &ContextSourceRef,
        order: i32,
    ) -> Result<ContextFragment, InjectionError> {
        Ok(ContextFragment {
            slot: fragment_slot(&source.slot).to_string(),
            label: fragment_label(&source.kind).to_string(),
            order,
            strategy: MergeStrategy::Append,
            scope: ContextFragment::default_scope(),
            source: "legacy:source_resolver:manual_text".to_string(),
            content: render_source_section(source, source.locator.clone()),
        })
    }
}

fn render_source_section(source: &ContextSourceRef, content: String) -> String {
    let title = display_source_label(source);
    format!("## 来源: {title}\n{content}")
}

fn display_source_label(source: &ContextSourceRef) -> &str {
    source.label.as_deref().unwrap_or(source.locator.as_str())
}

fn fragment_label(kind: &ContextSourceKind) -> &'static str {
    match kind {
        ContextSourceKind::ManualText => "declared_manual_text",
        ContextSourceKind::File => "declared_file_source",
        ContextSourceKind::ProjectSnapshot => "declared_project_snapshot",
        ContextSourceKind::HttpFetch => "declared_http_fetch",
        ContextSourceKind::McpResource => "declared_mcp_resource",
        ContextSourceKind::EntityRef => "declared_entity_ref",
    }
}

fn fragment_slot(slot: &ContextSlot) -> &'static str {
    match slot {
        ContextSlot::Requirements => "requirements",
        ContextSlot::Constraints => "constraints",
        ContextSlot::Codebase => "codebase",
        ContextSlot::References => "references",
        ContextSlot::InstructionAppend => "instruction_append",
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::context_source::{
        ContextDelivery, ContextSlot, ContextSourceKind, ContextSourceRef,
    };

    use super::*;

    #[test]
    fn resolves_manual_text_source() {
        let result = resolve_declared_sources(ResolveSourcesRequest {
            sources: &[ContextSourceRef {
                kind: ContextSourceKind::ManualText,
                locator: "hello world".to_string(),
                label: Some("manual note".to_string()),
                slot: ContextSlot::Requirements,
                priority: 100,
                required: true,
                max_chars: None,
                delivery: ContextDelivery::Resource,
            }],
            base_order: 10,
        })
        .expect("manual text should resolve");

        assert_eq!(result.fragments.len(), 1);
        assert!(result.fragments[0].content.contains("manual note"));
        assert!(result.fragments[0].content.contains("hello world"));
    }
}
