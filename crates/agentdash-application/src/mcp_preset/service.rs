use uuid::Uuid;

use agentdash_domain::mcp_preset::{McpPreset, McpPresetRepository, McpServerDecl};

use super::definition::{BuiltinMcpPresetTemplate, list_builtin_mcp_preset_templates};
use super::error::McpPresetApplicationError;

/// MCP Preset 服务——围绕单聚合仓储封装 CRUD + builtin 保护 + 复制为 user 的用例。
pub struct McpPresetService<'a, R: ?Sized> {
    repo: &'a R,
}

/// 创建 user preset 的输入。
#[derive(Debug, Clone)]
pub struct CreateMcpPresetInput {
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub server_decl: McpServerDecl,
}

/// 更新 preset 的输入——可选字段，`None` 表示保持原值。
#[derive(Debug, Clone, Default)]
pub struct UpdateMcpPresetInput {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub server_decl: Option<McpServerDecl>,
}

/// 复制 preset（常用于「复制 builtin 为 user」）的输入。
#[derive(Debug, Clone)]
pub struct CloneMcpPresetInput {
    pub source_id: Uuid,
    pub new_name: String,
}

impl<'a, R: ?Sized> McpPresetService<'a, R>
where
    R: McpPresetRepository,
{
    pub fn new(repo: &'a R) -> Self {
        Self { repo }
    }

    /// 列出某 project 下所有 Preset（含 builtin 与 user）。
    pub async fn list(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<McpPreset>, McpPresetApplicationError> {
        Ok(self.repo.list_by_project(project_id).await?)
    }

    /// 获取单个 Preset。
    pub async fn get(
        &self,
        id: Uuid,
    ) -> Result<McpPreset, McpPresetApplicationError> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| McpPresetApplicationError::NotFound(format!("mcp_preset 不存在: {id}")))
    }

    /// 创建一个 user preset。内部做 name 非空 + 唯一性校验。
    pub async fn create(
        &self,
        input: CreateMcpPresetInput,
    ) -> Result<McpPreset, McpPresetApplicationError> {
        validate_name(&input.name)?;
        validate_server_decl(&input.server_decl)?;
        self.ensure_name_available(input.project_id, &input.name, None)
            .await?;

        let preset = McpPreset::new_user(
            input.project_id,
            input.name,
            input.description,
            input.server_decl,
        );
        self.repo.create(&preset).await?;
        Ok(preset)
    }

    /// 更新一个 Preset；builtin 来源拒绝修改。
    pub async fn update(
        &self,
        id: Uuid,
        input: UpdateMcpPresetInput,
    ) -> Result<McpPreset, McpPresetApplicationError> {
        let mut preset = self.get(id).await?;
        if preset.is_builtin() {
            return Err(McpPresetApplicationError::Conflict(format!(
                "mcp_preset `{}` 属于 builtin，无法直接编辑；请先复制为 user",
                preset.name
            )));
        }

        if let Some(name) = input.name {
            validate_name(&name)?;
            if name != preset.name {
                self.ensure_name_available(preset.project_id, &name, Some(preset.id))
                    .await?;
            }
            preset.name = name;
        }
        if let Some(description) = input.description {
            preset.description = description;
        }
        if let Some(server_decl) = input.server_decl {
            validate_server_decl(&server_decl)?;
            preset.server_decl = server_decl;
        }
        preset.touch();

        self.repo.update(&preset).await?;
        Ok(preset)
    }

    /// 删除一个 Preset；builtin 来源拒绝删除（只允许复制为 user）。
    pub async fn delete(&self, id: Uuid) -> Result<(), McpPresetApplicationError> {
        let preset = self.get(id).await?;
        if preset.is_builtin() {
            return Err(McpPresetApplicationError::Conflict(format!(
                "mcp_preset `{}` 属于 builtin，无法删除；请先复制为 user 并单独管理",
                preset.name
            )));
        }
        self.repo.delete(id).await?;
        Ok(())
    }

    /// 将任意 Preset 复制为 user preset——这是 builtin 被「覆盖」的唯一入口。
    pub async fn clone_as_user(
        &self,
        input: CloneMcpPresetInput,
    ) -> Result<McpPreset, McpPresetApplicationError> {
        let source = self.get(input.source_id).await?;
        validate_name(&input.new_name)?;
        self.ensure_name_available(source.project_id, &input.new_name, None)
            .await?;

        let preset = McpPreset::new_user(
            source.project_id,
            input.new_name,
            source.description.clone(),
            source.server_decl.clone(),
        );
        self.repo.create(&preset).await?;
        Ok(preset)
    }

    /// 为给定 project 一次性装载全部 builtin 模板。幂等：已存在的 builtin 会被更新。
    pub async fn bootstrap_builtins(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<McpPreset>, McpPresetApplicationError> {
        let templates = list_builtin_mcp_preset_templates()
            .map_err(McpPresetApplicationError::Internal)?;
        let mut results = Vec::with_capacity(templates.len());
        for template in templates {
            results.push(self.bootstrap_single_builtin(project_id, &template).await?);
        }
        Ok(results)
    }

    /// 为给定 project 装载指定 builtin key——对应前端「新增内置」按钮。
    pub async fn bootstrap_builtin_by_key(
        &self,
        project_id: Uuid,
        builtin_key: &str,
    ) -> Result<McpPreset, McpPresetApplicationError> {
        let template = super::definition::get_builtin_mcp_preset_template(builtin_key)
            .map_err(McpPresetApplicationError::Internal)?
            .ok_or_else(|| {
                McpPresetApplicationError::NotFound(format!(
                    "builtin MCP Preset 模板不存在: {builtin_key}"
                ))
            })?;
        self.bootstrap_single_builtin(project_id, &template).await
    }

    async fn bootstrap_single_builtin(
        &self,
        project_id: Uuid,
        template: &BuiltinMcpPresetTemplate,
    ) -> Result<McpPreset, McpPresetApplicationError> {
        let preset = template.instantiate(project_id);
        // upsert_builtin 按 (project_id, builtin_key) 幂等处理——见 repository 实现。
        Ok(self.repo.upsert_builtin(&preset).await?)
    }

    async fn ensure_name_available(
        &self,
        project_id: Uuid,
        name: &str,
        allow_id: Option<Uuid>,
    ) -> Result<(), McpPresetApplicationError> {
        if let Some(existing) = self.repo.get_by_project_and_name(project_id, name).await?
            && Some(existing.id) != allow_id
        {
            return Err(McpPresetApplicationError::Conflict(format!(
                "mcp_preset name 已存在: {name}"
            )));
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<(), McpPresetApplicationError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(McpPresetApplicationError::BadRequest(
            "mcp_preset.name 不能为空".to_string(),
        ));
    }
    if trimmed.len() > 128 {
        return Err(McpPresetApplicationError::BadRequest(
            "mcp_preset.name 超过 128 字符".to_string(),
        ));
    }
    Ok(())
}

fn validate_server_decl(decl: &McpServerDecl) -> Result<(), McpPresetApplicationError> {
    if decl.server_name().trim().is_empty() {
        return Err(McpPresetApplicationError::BadRequest(
            "mcp_preset.server_decl.name 不能为空".to_string(),
        ));
    }
    match decl {
        McpServerDecl::Http { url, .. } | McpServerDecl::Sse { url, .. } => {
            if url.trim().is_empty() {
                return Err(McpPresetApplicationError::BadRequest(
                    "mcp_preset.server_decl.url 不能为空".to_string(),
                ));
            }
        }
        McpServerDecl::Stdio { command, .. } => {
            if command.trim().is_empty() {
                return Err(McpPresetApplicationError::BadRequest(
                    "mcp_preset.server_decl.command 不能为空".to_string(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use agentdash_domain::DomainError;
    use agentdash_domain::mcp_preset::McpPresetSource;

    use super::*;

    #[derive(Default)]
    struct InMemoryRepo {
        rows: Mutex<BTreeMap<Uuid, McpPreset>>,
    }

    impl InMemoryRepo {
        fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<Uuid, McpPreset>> {
            self.rows.lock().expect("preset repo lock poisoned")
        }
    }

    #[async_trait::async_trait]
    impl McpPresetRepository for InMemoryRepo {
        async fn create(&self, preset: &McpPreset) -> Result<(), DomainError> {
            let mut guard = self.lock();
            if guard
                .values()
                .any(|p| p.project_id == preset.project_id && p.name == preset.name)
            {
                return Err(DomainError::InvalidConfig(
                    "mcp_presets unique(project_id,name) violation".to_string(),
                ));
            }
            guard.insert(preset.id, preset.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<McpPreset>, DomainError> {
            Ok(self.lock().get(&id).cloned())
        }

        async fn get_by_project_and_name(
            &self,
            project_id: Uuid,
            name: &str,
        ) -> Result<Option<McpPreset>, DomainError> {
            Ok(self
                .lock()
                .values()
                .find(|p| p.project_id == project_id && p.name == name)
                .cloned())
        }

        async fn list_by_project(
            &self,
            project_id: Uuid,
        ) -> Result<Vec<McpPreset>, DomainError> {
            Ok(self
                .lock()
                .values()
                .filter(|p| p.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, preset: &McpPreset) -> Result<(), DomainError> {
            let mut guard = self.lock();
            if !guard.contains_key(&preset.id) {
                return Err(DomainError::NotFound {
                    entity: "mcp_preset",
                    id: preset.id.to_string(),
                });
            }
            guard.insert(preset.id, preset.clone());
            Ok(())
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            if self.lock().remove(&id).is_none() {
                return Err(DomainError::NotFound {
                    entity: "mcp_preset",
                    id: id.to_string(),
                });
            }
            Ok(())
        }

        async fn upsert_builtin(&self, preset: &McpPreset) -> Result<McpPreset, DomainError> {
            let key = match &preset.source {
                McpPresetSource::Builtin { key } => key.clone(),
                McpPresetSource::User => {
                    return Err(DomainError::InvalidConfig(
                        "upsert_builtin 仅接受 source=builtin".to_string(),
                    ));
                }
            };
            let mut guard = self.lock();
            let existing_id = guard
                .values()
                .find(|p| p.project_id == preset.project_id
                    && p.source.builtin_key() == Some(key.as_str()))
                .map(|p| p.id);
            let mut merged = preset.clone();
            if let Some(id) = existing_id {
                merged.id = id;
            }
            guard.insert(merged.id, merged.clone());
            Ok(merged)
        }
    }

    fn http_decl(name: &str) -> McpServerDecl {
        McpServerDecl::Http {
            name: name.to_string(),
            url: "https://example.com/mcp".to_string(),
            headers: vec![],
            relay: None,
        }
    }

    #[tokio::test]
    async fn create_and_list_works() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();

        service
            .create(CreateMcpPresetInput {
                project_id,
                name: "one".to_string(),
                description: None,
                server_decl: http_decl("srv"),
            })
            .await
            .expect("create");

        let listed = service.list(project_id).await.expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "one");
    }

    #[tokio::test]
    async fn create_rejects_duplicate_name_in_project() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();
        let input = || CreateMcpPresetInput {
            project_id,
            name: "dup".to_string(),
            description: None,
            server_decl: http_decl("srv"),
        };
        service.create(input()).await.expect("first");
        let err = service.create(input()).await.expect_err("duplicate");
        match err {
            McpPresetApplicationError::Conflict(msg) => assert!(msg.contains("已存在")),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_rejects_empty_name_and_invalid_decl() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();
        let err = service
            .create(CreateMcpPresetInput {
                project_id,
                name: "   ".to_string(),
                description: None,
                server_decl: http_decl("srv"),
            })
            .await
            .expect_err("blank name should fail");
        assert!(matches!(err, McpPresetApplicationError::BadRequest(_)));

        let err = service
            .create(CreateMcpPresetInput {
                project_id,
                name: "x".to_string(),
                description: None,
                server_decl: McpServerDecl::Stdio {
                    name: "srv".to_string(),
                    command: "".to_string(),
                    args: vec![],
                    env: vec![],
                    relay: None,
                },
            })
            .await
            .expect_err("empty command should fail");
        assert!(matches!(err, McpPresetApplicationError::BadRequest(_)));
    }

    #[tokio::test]
    async fn update_rejects_builtin() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();

        let builtins = service
            .bootstrap_builtins(project_id)
            .await
            .expect("bootstrap");
        assert!(!builtins.is_empty());

        let err = service
            .update(
                builtins[0].id,
                UpdateMcpPresetInput {
                    name: Some("new-name".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("builtin should reject update");
        assert!(matches!(err, McpPresetApplicationError::Conflict(_)));
    }

    #[tokio::test]
    async fn delete_rejects_builtin() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();
        let builtins = service
            .bootstrap_builtins(project_id)
            .await
            .expect("bootstrap");
        let err = service
            .delete(builtins[0].id)
            .await
            .expect_err("builtin should reject delete");
        assert!(matches!(err, McpPresetApplicationError::Conflict(_)));
    }

    #[tokio::test]
    async fn clone_as_user_creates_editable_copy() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();
        let builtins = service
            .bootstrap_builtins(project_id)
            .await
            .expect("bootstrap");

        let source = &builtins[0];
        let cloned = service
            .clone_as_user(CloneMcpPresetInput {
                source_id: source.id,
                new_name: format!("{}-copy", source.name),
            })
            .await
            .expect("clone");

        assert_eq!(cloned.source, McpPresetSource::User);
        assert_ne!(cloned.id, source.id);
        assert_eq!(cloned.server_decl, source.server_decl);

        // 复制出的 user preset 可被编辑
        let updated = service
            .update(
                cloned.id,
                UpdateMcpPresetInput {
                    description: Some(Some("updated".to_string())),
                    ..Default::default()
                },
            )
            .await
            .expect("user preset should be editable");
        assert_eq!(updated.description.as_deref(), Some("updated"));
    }

    #[tokio::test]
    async fn bootstrap_builtins_is_idempotent() {
        let repo = InMemoryRepo::default();
        let service = McpPresetService::new(&repo);
        let project_id = Uuid::new_v4();
        let first = service
            .bootstrap_builtins(project_id)
            .await
            .expect("first");
        let second = service
            .bootstrap_builtins(project_id)
            .await
            .expect("second");
        assert_eq!(first.len(), second.len());
        let listed = service.list(project_id).await.expect("list");
        assert_eq!(listed.len(), first.len());
    }
}
