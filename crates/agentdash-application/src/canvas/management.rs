use std::collections::BTreeSet;

use uuid::Uuid;

use agentdash_domain::DomainError;
use agentdash_domain::canvas::{Canvas, CanvasDataBinding, CanvasFile, CanvasSandboxConfig};

#[derive(Debug, Clone, Default)]
pub struct CanvasMutationInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub entry_file: Option<String>,
    pub sandbox_config: Option<CanvasSandboxConfig>,
    pub files: Option<Vec<CanvasFile>>,
    pub bindings: Option<Vec<CanvasDataBinding>>,
}

pub fn build_canvas(
    project_id: Uuid,
    title: String,
    description: String,
    input: CanvasMutationInput,
) -> Result<Canvas, DomainError> {
    let mut canvas = Canvas::new(project_id, title, description);
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
        if !binding_aliases.insert(binding.alias.clone()) {
            return Err(DomainError::InvalidConfig(format!(
                "Canvas binding alias 重复: {}",
                binding.alias
            )));
        }
        let binding_path = format!("bindings/{}.json", binding.alias);
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
    canvas.title = canvas.title.trim().to_string();
    canvas.description = canvas.description.trim().to_string();
    canvas.entry_file = normalize_path(&canvas.entry_file)?;

    for file in &mut canvas.files {
        file.path = normalize_path(&file.path)?;
    }
    for binding in &mut canvas.bindings {
        binding.alias = binding.alias.trim().to_string();
        binding.source_uri = binding.source_uri.trim().to_string();
        binding.content_type = binding.content_type.trim().to_string();
        if binding.content_type.is_empty() {
            binding.content_type = "application/json".to_string();
        }
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
            "Demo".to_string(),
            String::new(),
            CanvasMutationInput::default(),
        )
        .expect("应能创建 canvas");

        assert_eq!(canvas.entry_file, "src/main.tsx");
        assert_eq!(canvas.files.len(), 1);
        assert!(
            canvas
                .sandbox_config
                .libraries
                .contains(&"react".to_string())
        );
    }

    #[test]
    fn validate_canvas_contract_rejects_missing_entry_file() {
        let mut canvas = Canvas::new(Uuid::new_v4(), "Demo".to_string(), String::new());
        canvas.entry_file = "src/missing.tsx".to_string();

        let err = validate_canvas_contract(&canvas).expect_err("应拒绝缺失 entry");
        assert!(err.to_string().contains("必须存在于 files 中"));
    }
}
