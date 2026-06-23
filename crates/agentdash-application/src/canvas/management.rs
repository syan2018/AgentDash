use std::collections::BTreeSet;

use uuid::Uuid;

use crate::error::ApplicationError;
use crate::repository_set::RepositorySet;
use agentdash_domain::DomainError;
use agentdash_domain::canvas::{
    Canvas, CanvasDataBinding, CanvasFile, CanvasSandboxConfig,
    is_text_compatible_binding_content_type, normalize_binding_content_type,
};

use super::{derive_canvas_mount_id, normalize_canvas_mount_id};

#[derive(Debug, Clone, Default)]
pub struct CanvasMutationInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
    pub bindings: Option<Vec<CanvasDataBinding>>,
}

#[derive(Debug, Clone)]
pub struct CreateCanvasInput {
    pub project_id: Uuid,
    pub mount_id: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub mutation: CanvasMutationInput,
}

pub async fn list_project_canvases(
    repos: &RepositorySet,
    project_id: Uuid,
) -> Result<Vec<Canvas>, ApplicationError> {
    repos
        .canvas_repo
        .list_by_project(project_id)
        .await
        .map_err(ApplicationError::from)
}

pub async fn create_project_canvas(
    repos: &RepositorySet,
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
        .canvas_repo
        .create(&canvas)
        .await
        .map_err(ApplicationError::from)?;
    Ok(canvas)
}

pub async fn load_canvas_by_id(
    repos: &RepositorySet,
    canvas_id: Uuid,
) -> Result<Canvas, ApplicationError> {
    let canvas = repos
        .canvas_repo
        .get_by_id(canvas_id)
        .await
        .map_err(ApplicationError::from)?;

    canvas.ok_or_else(|| ApplicationError::NotFound(format!("Canvas {canvas_id} 不存在")))
}

pub async fn load_canvas_by_project_mount_id(
    repos: &RepositorySet,
    project_id: Uuid,
    raw_canvas_mount_id: &str,
) -> Result<Canvas, ApplicationError> {
    let canvas_mount_id = normalize_canvas_mount_id(raw_canvas_mount_id)
        .map_err(|error| ApplicationError::BadRequest(error.to_string()))?;
    let canvas = repos
        .canvas_repo
        .get_by_mount_id(project_id, &canvas_mount_id)
        .await
        .map_err(ApplicationError::from)?;

    canvas
        .ok_or_else(|| ApplicationError::NotFound(format!("Canvas mount {canvas_mount_id} 不存在")))
}

pub async fn update_canvas_record(
    repos: &RepositorySet,
    mut canvas: Canvas,
    input: CanvasMutationInput,
) -> Result<Canvas, ApplicationError> {
    apply_canvas_mutation(&mut canvas, input).map_err(ApplicationError::from)?;
    repos
        .canvas_repo
        .update(&canvas)
        .await
        .map_err(ApplicationError::from)?;
    Ok(canvas)
}

pub async fn delete_canvas_record(
    repos: &RepositorySet,
    canvas: &Canvas,
) -> Result<(), ApplicationError> {
    repos
        .canvas_repo
        .delete(canvas.id)
        .await
        .map_err(ApplicationError::from)
}

pub fn build_canvas(
    project_id: Uuid,
    mount_id: Option<String>,
    title: String,
    description: String,
    input: CanvasMutationInput,
) -> Result<Canvas, DomainError> {
    let mount_id = match mount_id {
        Some(value) => normalize_canvas_mount_id(&value)?,
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
    if let Some(bindings) = input.bindings {
        canvas.bindings = bindings;
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

    let mut binding_aliases = BTreeSet::new();
    for binding in &canvas.bindings {
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

pub fn upsert_canvas_binding(
    canvas: &mut Canvas,
    binding: CanvasDataBinding,
) -> Result<(), DomainError> {
    if let Some(existing) = canvas
        .bindings
        .iter_mut()
        .find(|item| item.alias == binding.alias)
    {
        *existing = binding;
    } else {
        canvas.bindings.push(binding);
    }
    normalize_canvas(canvas)?;
    validate_canvas_contract(canvas)?;
    canvas.touch();
    Ok(())
}

fn normalize_canvas(canvas: &mut Canvas) -> Result<(), DomainError> {
    canvas.mount_id = normalize_canvas_mount_id(&canvas.mount_id)?;
    canvas.title = canvas.title.trim().to_string();
    canvas.description = canvas.description.trim().to_string();
    canvas.entry_file = normalize_path(&canvas.entry_file)?;

    for file in &mut canvas.files {
        file.path = normalize_path(&file.path)?;
    }
    for binding in &mut canvas.bindings {
        binding.alias = binding.alias.trim().to_string();
        binding.source_uri = binding.source_uri.trim().to_string();
        binding.content_type =
            normalize_binding_content_type(Some(&binding.content_type), &binding.source_uri);
    }

    Ok(())
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
    fn derived_canvas_mount_id_uses_cvs_prefix_once() {
        assert_eq!(
            derive_canvas_mount_id("Demo Dashboard"),
            "cvs-demo-dashboard"
        );
        assert_eq!(derive_canvas_mount_id("cvs-demo"), "cvs-demo");
    }

    #[test]
    fn validate_canvas_contract_rejects_binary_data_binding() {
        let mut canvas = build_canvas(
            Uuid::new_v4(),
            Some("cvs-demo".to_string()),
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建 canvas");
        canvas.bindings = vec![CanvasDataBinding::with_content_type(
            "logo".to_string(),
            "main://assets/logo.png".to_string(),
            Some("image/png".to_string()),
        )];

        let err = validate_canvas_contract(&canvas).expect_err("应拒绝非文本绑定");
        assert!(err.to_string().contains("不是文本数据类型"));
    }
}
