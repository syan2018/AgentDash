use std::collections::BTreeSet;

use uuid::Uuid;

use crate::error::ApplicationError;
use agentdash_domain::DomainError;
use agentdash_domain::canvas::{
    Canvas, CanvasAccessAction, CanvasAccessProjection, CanvasDataBinding, CanvasFile,
    CanvasRepository, CanvasSandboxConfig, CanvasScope, canvas_access_projection,
    is_text_compatible_binding_content_type, normalize_binding_content_type,
};
use agentdash_domain::project::{
    ProjectAuthorization, ProjectAuthorizationContext, ProjectAuthorizationService,
    ProjectPermission, ProjectRepository,
};

use super::{derive_canvas_mount_id, normalize_canvas_mount_id};

pub trait CanvasRepositorySet: Send + Sync {
    fn project_repo(&self) -> &dyn ProjectRepository;
    fn canvas_repo(&self) -> &dyn CanvasRepository;
}

#[derive(Debug, Clone, Default)]
pub struct CanvasMutationInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
}

#[derive(Debug, Clone)]
pub struct CreateCanvasInput {
    pub project_id: Uuid,
    pub mount_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub mutation: CanvasMutationInput,
}

#[derive(Debug, Clone)]
pub struct CreatePersonalCanvasInput {
    pub project_id: Uuid,
    pub mount_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub mutation: CanvasMutationInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CanvasListScopeFilter {
    #[default]
    All,
    Mine,
    Shared,
}

#[derive(Debug, Clone)]
pub struct CanvasWithAccess {
    pub canvas: Canvas,
    pub access: CanvasAccessProjection,
}

#[derive(Debug, Clone, Default)]
pub struct PublishCanvasInput {
    pub mount_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CopyCanvasInput {
    pub mount_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnpublishCanvasResult {
    pub unpublished_canvas_id: Uuid,
    pub source_canvas_id: Option<Uuid>,
}

pub async fn list_project_canvases(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
) -> Result<Vec<Canvas>, ApplicationError> {
    repos
        .canvas_repo()
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn create_personal_canvas(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    input: CreatePersonalCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    let (_project_access_project, project_access) = require_project_access(
        repos.project_repo(),
        current_user,
        input.project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let title = input.title.trim();
    if title.is_empty() {
        return Err(ApplicationError::BadRequest(
            "Canvas 标题不能为空".to_string(),
        ));
    }

    let canvas = build_personal_canvas(
        input.project_id,
        current_user.user_id.clone(),
        input.mount_id,
        title.to_string(),
        input.description.unwrap_or_default(),
        input.mutation,
    )
    .map_err(ApplicationError::from)?;

    repos
        .canvas_repo()
        .create(&canvas)
        .await
        .map_err(ApplicationError::from)?;

    Ok(CanvasWithAccess {
        access: canvas_access_projection(&canvas, current_user, &project_access),
        canvas,
    })
}

pub async fn create_project_canvas(
    repos: &dyn CanvasRepositorySet,
    input: CreateCanvasInput,
) -> Result<Canvas, ApplicationError> {
    let title = input.title.trim();
    if title.is_empty() {
        return Err(ApplicationError::BadRequest(
            "Canvas 标题不能为空".to_string(),
        ));
    }

    let canvas = build_canvas(
        input.project_id,
        input.mount_id,
        title.to_string(),
        input.description.unwrap_or_default(),
        input.mutation,
    )
    .map_err(ApplicationError::from)?;
    repos
        .canvas_repo()
        .create(&canvas)
        .await
        .map_err(ApplicationError::from)?;
    Ok(canvas)
}

pub async fn list_canvases_for_user(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    project_id: Uuid,
    filter: CanvasListScopeFilter,
) -> Result<Vec<CanvasWithAccess>, ApplicationError> {
    let (_project, project_access) = require_project_access(
        repos.project_repo(),
        current_user,
        project_id,
        ProjectPermission::View,
    )
    .await?;

    let mut canvases = match filter {
        CanvasListScopeFilter::All => {
            let mut canvases = repos
                .canvas_repo()
                .list_personal_by_owner(project_id, &current_user.user_id)
                .await
                .map_err(ApplicationError::from)?;
            canvases.extend(
                repos
                    .canvas_repo()
                    .list_project_shared(project_id)
                    .await
                    .map_err(ApplicationError::from)?,
            );
            canvases
        }
        CanvasListScopeFilter::Mine => repos
            .canvas_repo()
            .list_personal_by_owner(project_id, &current_user.user_id)
            .await
            .map_err(ApplicationError::from)?,
        CanvasListScopeFilter::Shared => repos
            .canvas_repo()
            .list_project_shared(project_id)
            .await
            .map_err(ApplicationError::from)?,
    };

    canvases.sort_by(|left, right| right.created_at.cmp(&left.created_at));

    Ok(canvases
        .into_iter()
        .filter_map(|canvas| {
            let access = canvas_access_projection(&canvas, current_user, &project_access);
            access
                .can_view
                .then_some(CanvasWithAccess { canvas, access })
        })
        .collect())
}

pub async fn load_canvas_by_id(
    repos: &dyn CanvasRepositorySet,
    canvas_id: Uuid,
) -> Result<Canvas, ApplicationError> {
    let canvas = repos
        .canvas_repo()
        .get_by_id(canvas_id)
        .await
        .map_err(ApplicationError::from)?;

    canvas.ok_or_else(|| ApplicationError::NotFound(format!("Canvas {canvas_id} 不存在")))
}

pub async fn load_canvas_with_access(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    canvas_id: Uuid,
    required_action: CanvasAccessAction,
) -> Result<CanvasWithAccess, ApplicationError> {
    let canvas = load_canvas_by_id(repos, canvas_id).await?;
    let (_project, project_access) = require_project_access(
        repos.project_repo(),
        current_user,
        canvas.project_id,
        ProjectPermission::View,
    )
    .await?;
    let access = canvas_access_projection(&canvas, current_user, &project_access);
    require_canvas_action(&access, required_action, canvas.id)?;
    Ok(CanvasWithAccess { canvas, access })
}

pub async fn load_canvas_by_project_mount_id(
    repos: &dyn CanvasRepositorySet,
    project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, ApplicationError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| ApplicationError::BadRequest(error.to_string()))?;
    let canvas = repos
        .canvas_repo()
        .get_by_mount_id(project_id, &canvas_mount_id)
        .await
        .map_err(ApplicationError::from)?;

    canvas
        .ok_or_else(|| ApplicationError::NotFound(format!("Canvas mount {canvas_mount_id} 不存在")))
}

pub async fn publish_canvas_to_project(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    source_canvas_id: Uuid,
    input: PublishCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    let CanvasWithAccess {
        canvas: mut source, ..
    } = load_canvas_with_access(
        repos,
        current_user,
        source_canvas_id,
        CanvasAccessAction::Publish,
    )
    .await?;

    if source.scope != CanvasScope::Personal {
        return Err(ApplicationError::BadRequest(
            "只有个人 Canvas 可以发布到项目共用区".to_string(),
        ));
    }

    let (_project, project_access) = require_project_access(
        repos.project_repo(),
        current_user,
        source.project_id,
        ProjectPermission::Edit,
    )
    .await?;

    let mut shared = if let Some(existing) = repos
        .canvas_repo()
        .find_published_from(source.id)
        .await
        .map_err(ApplicationError::from)?
    {
        existing
    } else {
        let base_mount_id = input
            .mount_id
            .clone()
            .unwrap_or_else(|| format!("{}-shared", source.mount_id));
        let mount_id =
            unique_canvas_mount_id(repos.canvas_repo(), source.project_id, &base_mount_id).await?;
        Canvas::new_project_shared(
            source.project_id,
            mount_id,
            source.title.clone(),
            source.description.clone(),
            Some(source.id),
            Some(current_user.user_id.clone()),
        )
    };

    if shared.scope != CanvasScope::Project {
        return Err(ApplicationError::Conflict(format!(
            "Canvas {} 的发布记录不是项目共用 Canvas",
            shared.id
        )));
    }

    replace_canvas_authoring_payload(&source, &mut shared);
    shared.scope = CanvasScope::Project;
    shared.owner_user_id = Some(current_user.user_id.clone());
    shared.published_from_canvas_id = Some(source.id);
    shared.published_at = Some(chrono::Utc::now());
    shared.published_by_user_id = Some(current_user.user_id.clone());
    apply_canvas_text_overrides(&mut shared, input.title, input.description);
    normalize_canvas(&mut shared).map_err(ApplicationError::from)?;
    validate_canvas_contract(&shared).map_err(ApplicationError::from)?;

    if source.shared_canvas_id == Some(shared.id) {
        repos
            .canvas_repo()
            .update(&shared)
            .await
            .map_err(ApplicationError::from)?;
    } else if repos
        .canvas_repo()
        .get_by_id(shared.id)
        .await
        .map_err(ApplicationError::from)?
        .is_some()
    {
        repos
            .canvas_repo()
            .update(&shared)
            .await
            .map_err(ApplicationError::from)?;
    } else {
        repos
            .canvas_repo()
            .create(&shared)
            .await
            .map_err(ApplicationError::from)?;
    }

    source.shared_canvas_id = Some(shared.id);
    source.touch();
    repos
        .canvas_repo()
        .update(&source)
        .await
        .map_err(ApplicationError::from)?;

    Ok(CanvasWithAccess {
        access: canvas_access_projection(&shared, current_user, &project_access),
        canvas: shared,
    })
}

pub async fn copy_canvas_to_personal(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    source_canvas_id: Uuid,
    input: CopyCanvasInput,
) -> Result<CanvasWithAccess, ApplicationError> {
    let CanvasWithAccess { canvas: source, .. } = load_canvas_with_access(
        repos,
        current_user,
        source_canvas_id,
        CanvasAccessAction::Copy,
    )
    .await?;

    let (_project, project_access) = require_project_access(
        repos.project_repo(),
        current_user,
        source.project_id,
        ProjectPermission::View,
    )
    .await?;

    let mount_id = if let Some(base_mount_id) = input.mount_id.clone() {
        unique_canvas_mount_id(repos.canvas_repo(), source.project_id, &base_mount_id).await?
    } else {
        unique_copy_canvas_mount_id(repos.canvas_repo(), source.project_id, &source.mount_id)
            .await?
    };
    let mut copy = Canvas::new_personal(
        source.project_id,
        current_user.user_id.clone(),
        mount_id,
        source.title.clone(),
        source.description.clone(),
    );
    replace_canvas_authoring_payload(&source, &mut copy);
    copy.cloned_from_canvas_id = Some(source.id);
    apply_canvas_text_overrides(&mut copy, input.title, input.description);
    normalize_canvas(&mut copy).map_err(ApplicationError::from)?;
    validate_canvas_contract(&copy).map_err(ApplicationError::from)?;

    repos
        .canvas_repo()
        .create(&copy)
        .await
        .map_err(ApplicationError::from)?;

    Ok(CanvasWithAccess {
        access: canvas_access_projection(&copy, current_user, &project_access),
        canvas: copy,
    })
}

pub async fn unpublish_project_canvas(
    repos: &dyn CanvasRepositorySet,
    current_user: &ProjectAuthorizationContext,
    shared_canvas_id: Uuid,
) -> Result<UnpublishCanvasResult, ApplicationError> {
    let CanvasWithAccess { canvas: shared, .. } = load_canvas_with_access(
        repos,
        current_user,
        shared_canvas_id,
        CanvasAccessAction::ManageShared,
    )
    .await?;

    if shared.scope != CanvasScope::Project {
        return Err(ApplicationError::BadRequest(
            "只有项目共用 Canvas 可以取消发布".to_string(),
        ));
    }

    if let Some(source_id) = shared.published_from_canvas_id {
        if let Some(mut source) = repos
            .canvas_repo()
            .get_by_id(source_id)
            .await
            .map_err(ApplicationError::from)?
        {
            if source.shared_canvas_id == Some(shared.id) {
                source.shared_canvas_id = None;
                source.touch();
                repos
                    .canvas_repo()
                    .update(&source)
                    .await
                    .map_err(ApplicationError::from)?;
            }
        }
    }

    repos
        .canvas_repo()
        .delete(shared.id)
        .await
        .map_err(ApplicationError::from)?;

    Ok(UnpublishCanvasResult {
        unpublished_canvas_id: shared.id,
        source_canvas_id: shared.published_from_canvas_id,
    })
}

pub async fn update_canvas_record(
    repos: &dyn CanvasRepositorySet,
    mut canvas: Canvas,
    input: CanvasMutationInput,
) -> Result<Canvas, ApplicationError> {
    apply_canvas_mutation(&mut canvas, input).map_err(ApplicationError::from)?;
    repos
        .canvas_repo()
        .update(&canvas)
        .await
        .map_err(ApplicationError::from)?;
    Ok(canvas)
}

pub async fn delete_canvas_record(
    repos: &dyn CanvasRepositorySet,
    canvas: &Canvas,
) -> Result<(), ApplicationError> {
    repos
        .canvas_repo()
        .delete(canvas.id)
        .await
        .map_err(ApplicationError::from)
}

pub fn build_personal_canvas(
    project_id: Uuid,
    owner_user_id: String,
    mount_id: Option<String>,
    title: String,
    description: String,
    input: CanvasMutationInput,
) -> Result<Canvas, DomainError> {
    let mount_id = match mount_id {
        Some(value) => normalize_canvas_mount_id_for_domain(&value)?,
        None => derive_canvas_mount_id(&title),
    };
    let mut canvas = Canvas::new_personal(project_id, owner_user_id, mount_id, title, description);
    canvas.sandbox_config = CanvasSandboxConfig::react_default();
    apply_canvas_mutation(&mut canvas, input)?;
    validate_canvas_contract(&canvas)?;
    Ok(canvas)
}

pub fn build_canvas(
    project_id: Uuid,
    mount_id: Option<String>,
    title: String,
    description: String,
    input: CanvasMutationInput,
) -> Result<Canvas, DomainError> {
    let mount_id = match mount_id {
        Some(value) => normalize_canvas_mount_id_for_domain(&value)?,
        None => derive_canvas_mount_id(&title),
    };
    let mut canvas = Canvas::new(project_id, mount_id, title, description);
    canvas.sandbox_config = CanvasSandboxConfig::react_default();
    apply_canvas_mutation(&mut canvas, input)?;
    validate_canvas_contract(&canvas)?;
    Ok(canvas)
}

pub fn apply_canvas_mutation(
    canvas: &mut Canvas,
    input: CanvasMutationInput,
) -> Result<(), DomainError> {
    if let Some(title) = input.title {
        canvas.title = title;
    }
    if let Some(description) = input.description {
        canvas.description = description;
    }
    if let Some(entry_file) = input.entry_file {
        canvas.entry_file = entry_file;
    }
    if let Some(sandbox_config) = input.sandbox_config {
        canvas.sandbox_config = sandbox_config;
    }
    if let Some(files) = input.files {
        canvas.files = files;
    }
    normalize_canvas(canvas)?;
    validate_canvas_contract(canvas)?;
    canvas.touch();
    Ok(())
}

pub fn validate_canvas_contract(canvas: &Canvas) -> Result<(), DomainError> {
    if canvas.mount_id.trim().is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas mount_id 不能为空".to_string(),
        ));
    }
    if canvas.scope == CanvasScope::Personal && canvas.owner_user_id.is_none() {
        return Err(DomainError::InvalidConfig(
            "个人 Canvas 必须有 owner_user_id".to_string(),
        ));
    }
    if canvas.title.trim().is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas 标题不能为空".to_string(),
        ));
    }
    if canvas.entry_file.trim().is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas entry_file 不能为空".to_string(),
        ));
    }
    if canvas.files.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas 至少需要一个文件".to_string(),
        ));
    }

    let mut file_paths = BTreeSet::new();
    let mut has_entry = false;
    for file in &canvas.files {
        if file.path.trim().is_empty() {
            return Err(DomainError::InvalidConfig(
                "Canvas 文件路径不能为空".to_string(),
            ));
        }
        if !file_paths.insert(file.path.clone()) {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas 文件路径重复: {}",
                file.path
            )));
        }
        if file.path == canvas.entry_file {
            has_entry = true;
        }
    }
    if !has_entry {
        return Err(DomainError::InvalidConfig(format!(
            "Canvas entry_file `{}` 必须存在于 files 中",
            canvas.entry_file
        )));
    }

    Ok(())
}

pub fn upsert_canvas_data_binding(
    bindings: &mut Vec<CanvasDataBinding>,
    binding: CanvasDataBinding,
) -> Result<(), DomainError> {
    let mut binding = binding;
    normalize_canvas_data_binding(&mut binding);
    if let Some(existing) = bindings.iter_mut().find(|item| item.alias == binding.alias) {
        *existing = binding;
    } else {
        bindings.push(binding);
    }
    Ok(())
}

pub fn validate_canvas_data_bindings(
    canvas: &Canvas,
    bindings: &[CanvasDataBinding],
) -> Result<(), DomainError> {
    let file_paths = canvas
        .files
        .iter()
        .map(|file| file.path.clone())
        .collect::<BTreeSet<_>>();
    let mut binding_aliases = BTreeSet::new();
    for binding in bindings {
        if binding.alias.trim().is_empty() {
            return Err(DomainError::InvalidConfig(
                "Canvas binding alias 不能为空".to_string(),
            ));
        }
        if binding.alias.contains('/') || binding.alias.contains('\\') {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas binding alias 不能包含路径分隔符: {}",
                binding.alias
            )));
        }
        if binding.source_uri.trim().is_empty() {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas binding `{}` 的 source_uri 不能为空",
                binding.alias
            )));
        }
        if !is_text_compatible_binding_content_type(&binding.content_type) {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas binding `{}` 的 content_type `{}` 不是文本数据类型",
                binding.alias, binding.content_type
            )));
        }
        if !binding_aliases.insert(binding.alias.clone()) {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas binding alias 重复: {}",
                binding.alias
            )));
        }
        let binding_path = binding.data_path();
        if file_paths.contains(&binding_path) {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas binding `{}` 会与已有文件路径冲突: {}",
                binding.alias, binding_path
            )));
        }
    }

    Ok(())
}

fn normalize_canvas(canvas: &mut Canvas) -> Result<(), DomainError> {
    canvas.mount_id = normalize_canvas_mount_id_for_domain(&canvas.mount_id)?;
    canvas.owner_user_id = normalize_optional_text(canvas.owner_user_id.take());
    canvas.published_by_user_id = normalize_optional_text(canvas.published_by_user_id.take());
    canvas.title = canvas.title.trim().to_string();
    canvas.description = canvas.description.trim().to_string();
    canvas.entry_file = normalize_path(&canvas.entry_file)?;

    for file in &mut canvas.files {
        file.path = normalize_path(&file.path)?;
    }

    Ok(())
}

fn normalize_canvas_data_binding(binding: &mut CanvasDataBinding) {
    binding.alias = binding.alias.trim().to_string();
    binding.source_uri = binding.source_uri.trim().to_string();
    binding.content_type =
        normalize_binding_content_type(Some(&binding.content_type), &binding.source_uri);
}

fn normalize_canvas_mount_id_for_domain(raw: &str) -> Result<String, DomainError> {
    normalize_canvas_mount_id(raw).map_err(|error| DomainError::InvalidConfig(error.to_string()))
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn require_project_access(
    project_repo: &dyn ProjectRepository,
    current_user: &ProjectAuthorizationContext,
    project_id: Uuid,
    permission: ProjectPermission,
) -> Result<(agentdash_domain::project::Project, ProjectAuthorization), ApplicationError> {
    let project = project_repo
        .get_by_id(project_id)
        .await
        .map_err(ApplicationError::from)?
        .ok_or_else(|| ApplicationError::NotFound(format!("Project {project_id} 不存在")))?;
    let authz = ProjectAuthorizationService::new(project_repo);
    let access = authz
        .resolve_project_access(current_user, &project)
        .await
        .map_err(ApplicationError::from)?;
    if !access.allows(permission) {
        let action = match permission {
            ProjectPermission::View => "查看",
            ProjectPermission::Edit => "编辑",
            ProjectPermission::ManageSharing => "管理共享",
        };
        return Err(ApplicationError::Forbidden(format!(
            "当前用户无权{action} Project {project_id}"
        )));
    }
    Ok((project, access))
}

fn require_canvas_action(
    access: &CanvasAccessProjection,
    action: CanvasAccessAction,
    canvas_id: Uuid,
) -> Result<(), ApplicationError> {
    if access.allows(action) {
        return Ok(());
    }
    let action = match action {
        CanvasAccessAction::View => "查看",
        CanvasAccessAction::EditSource => "编辑",
        CanvasAccessAction::Publish => "发布",
        CanvasAccessAction::ManageShared => "管理共用发布",
        CanvasAccessAction::Copy => "复制",
        CanvasAccessAction::RuntimeWrite => "写入运行面",
    };
    Err(ApplicationError::Forbidden(format!(
        "当前用户无权{action} Canvas {canvas_id}"
    )))
}

fn replace_canvas_authoring_payload(source: &Canvas, target: &mut Canvas) {
    target.title = source.title.clone();
    target.description = source.description.clone();
    target.entry_file = source.entry_file.clone();
    target.sandbox_config = source.sandbox_config.clone();
    target.files = source.files.clone();
    target.touch();
}

fn apply_canvas_text_overrides(
    canvas: &mut Canvas,
    title: Option<String>,
    description: Option<String>,
) {
    if let Some(title) = title {
        canvas.title = title;
    }
    if let Some(description) = description {
        canvas.description = description;
    }
}

async fn unique_canvas_mount_id(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    raw_base_mount_id: &str,
) -> Result<String, ApplicationError> {
    let base_mount_id = normalize_canvas_mount_id(raw_base_mount_id)
        .map_err(|error| ApplicationError::BadRequest(error.to_string()))?;
    for suffix in 0..100 {
        let candidate = if suffix == 0 {
            base_mount_id.clone()
        } else {
            format!("{base_mount_id}-{suffix}")
        };
        if canvas_repo
            .get_by_mount_id(project_id, &candidate)
            .await
            .map_err(ApplicationError::from)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(ApplicationError::Conflict(format!(
        "无法为 Canvas mount `{base_mount_id}` 生成唯一标识"
    )))
}

async fn unique_copy_canvas_mount_id(
    canvas_repo: &dyn CanvasRepository,
    project_id: Uuid,
    source_mount_id: &str,
) -> Result<String, ApplicationError> {
    let source_mount_id = normalize_canvas_mount_id(source_mount_id)
        .map_err(|error| ApplicationError::BadRequest(error.to_string()))?;
    for _ in 0..100 {
        let candidate = format!("{source_mount_id}-copy-{}", random_copy_suffix());
        if canvas_repo
            .get_by_mount_id(project_id, &candidate)
            .await
            .map_err(ApplicationError::from)?
            .is_none()
        {
            return Ok(candidate);
        }
    }
    Err(ApplicationError::Conflict(format!(
        "无法为 Canvas mount `{source_mount_id}` 生成复制标识"
    )))
}

fn random_copy_suffix() -> String {
    const CHARS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let bytes = *Uuid::new_v4().as_bytes();
    (0..4)
        .map(|index| CHARS[usize::from(bytes[index]) % CHARS.len()] as char)
        .collect()
}

fn normalize_path(path: &str) -> Result<String, DomainError> {
    let normalized = path.trim().replace('\\', "/");
    let normalized = normalized.trim_matches('/').to_string();
    if normalized.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Canvas 路径不能为空".to_string(),
        ));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::project::ProjectRole;

    #[test]
    fn build_canvas_uses_react_default_and_seed_file() {
        let canvas = build_canvas(
            Uuid::new_v4(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建 canvas");

        assert_eq!(canvas.entry_file, "src/main.tsx");
        assert!(canvas.files.iter().any(|file| file.path == "src/main.tsx"));
        assert!(
            canvas
                .files
                .iter()
                .all(|file| !file.path.starts_with("skills/canvas-system/"))
        );
        assert!(
            canvas
                .sandbox_config
                .libraries
                .contains(&"react".to_string())
        );
    }

    #[test]
    fn build_personal_canvas_sets_owner_and_personal_scope() {
        let canvas = build_personal_canvas(
            Uuid::new_v4(),
            "alice".to_string(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建个人 canvas");

        assert_eq!(canvas.owner_user_id.as_deref(), Some("alice"));
        assert_eq!(canvas.scope, CanvasScope::Personal);
        assert!(canvas.published_from_canvas_id.is_none());
        assert!(canvas.cloned_from_canvas_id.is_none());
    }

    #[test]
    fn apply_canvas_mutation_replaces_source_files_without_system_skill_injection() {
        let mut canvas = build_canvas(
            Uuid::new_v4(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建 canvas");

        apply_canvas_mutation(
            &mut canvas,
            CanvasMutationInput {
                files: Some(vec![CanvasFile::new(
                    "src/main.tsx".to_string(),
                    "console.log('updated')".to_string(),
                )]),
                ..CanvasMutationInput::default()
            },
        )
        .expect("应能更新 canvas");

        assert!(
            canvas
                .files
                .iter()
                .all(|file| !file.path.starts_with("skills/canvas-system/"))
        );
    }

    #[test]
    fn validate_canvas_contract_rejects_missing_entry_file() {
        let mut canvas = Canvas::new(
            Uuid::new_v4(),
            "cvs-demo".to_string(),
            "Demo".to_string(),
            String::new(),
        );
        canvas.entry_file = "src/missing.tsx".to_string();

        let err = validate_canvas_contract(&canvas).expect_err("应拒绝缺失 entry");
        assert!(err.to_string().contains("必须存在于 files 中"));
    }

    #[test]
    fn canvas_access_projection_allows_owner_write_but_shared_is_runtime_read_only() {
        let current_user = ProjectAuthorizationContext::new("alice".to_string(), Vec::new(), false);
        let project_access = ProjectAuthorization {
            role: Some(ProjectRole::Editor),
            via_admin_bypass: false,
            via_template_visibility: false,
        };
        let personal = build_personal_canvas(
            Uuid::new_v4(),
            "alice".to_string(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建个人 canvas");
        let mut shared = Canvas::new_project_shared(
            personal.project_id,
            "cvs-demo-shared".to_string(),
            personal.title.clone(),
            personal.description.clone(),
            Some(personal.id),
            Some("alice".to_string()),
        );
        shared.copy_authoring_from(&personal);

        let personal_access = canvas_access_projection(&personal, &current_user, &project_access);
        assert!(personal_access.can_edit_source);
        assert!(personal_access.can_publish);
        assert!(personal_access.runtime_write_allowed);

        let shared_access = canvas_access_projection(&shared, &current_user, &project_access);
        assert!(shared_access.can_view);
        assert!(shared_access.can_copy);
        assert!(shared_access.can_manage_shared);
        assert!(!shared_access.can_edit_source);
        assert!(!shared_access.runtime_write_allowed);
    }

    #[test]
    fn canvas_access_projection_hides_other_users_personal_canvas() {
        let current_user = ProjectAuthorizationContext::new("bob".to_string(), Vec::new(), false);
        let project_access = ProjectAuthorization {
            role: Some(ProjectRole::Owner),
            via_admin_bypass: false,
            via_template_visibility: false,
        };
        let personal = build_personal_canvas(
            Uuid::new_v4(),
            "alice".to_string(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建个人 canvas");

        let access = canvas_access_projection(&personal, &current_user, &project_access);
        assert!(!access.can_view);
        assert!(!access.can_edit_source);
        assert!(!access.runtime_write_allowed);
    }

    #[test]
    fn derived_canvas_mount_id_uses_cvs_prefix_once() {
        assert_eq!(
            derive_canvas_mount_id("Demo Dashboard"),
            "cvs-demo-dashboard"
        );
        assert_eq!(derive_canvas_mount_id("cvs-demo"), "cvs-demo");
    }

    #[test]
    fn validate_canvas_data_bindings_rejects_binary_binding() {
        let canvas = build_canvas(
            Uuid::new_v4(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建 canvas");
        let bindings = vec![CanvasDataBinding::with_content_type(
            "logo".to_string(),
            "main://assets/logo.png".to_string(),
            Some("image/png".to_string()),
        )];

        let err = validate_canvas_data_bindings(&canvas, &bindings).expect_err("应拒绝非文本绑定");
        assert!(err.to_string().contains("不是文本数据类型"));
    }

    #[test]
    fn validate_canvas_contract_rejects_personal_canvas_without_owner() {
        let mut canvas = Canvas::new(
            Uuid::new_v4(),
            "cvs-demo".to_string(),
            "Demo".to_string(),
            String::new(),
        );
        canvas.scope = CanvasScope::Personal;

        let err = validate_canvas_contract(&canvas).expect_err("应拒绝缺少 owner");
        assert!(err.to_string().contains("owner_user_id"));
    }
}
