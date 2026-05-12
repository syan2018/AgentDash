use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::embedded_skill::EmbeddedSkillBundle;
use agentdash_domain::skill_asset::{
    SkillAsset, SkillAssetFile, SkillAssetFileKind, SkillAssetRepository,
};
use uuid::Uuid;

use crate::skill::parse_skill_file;

use super::definition::{
    BuiltinSkillAssetTemplate, get_builtin_skill_asset_template, list_builtin_skill_asset_templates,
};
use super::error::SkillAssetApplicationError;

const MAX_SKILL_KEY_LENGTH: usize = 64;
const MAX_SKILL_DESCRIPTION_LENGTH: usize = 1024;

#[derive(Debug, Clone)]
pub struct SkillAssetFileInput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct CreateSkillAssetInput {
    pub project_id: Uuid,
    pub key: String,
    pub display_name: String,
    pub description: String,
    pub disable_model_invocation: bool,
    pub files: Vec<SkillAssetFileInput>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateSkillAssetInput {
    pub key: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub disable_model_invocation: Option<bool>,
    pub files: Option<Vec<SkillAssetFileInput>>,
}

#[derive(Debug, Clone)]
pub struct RawSkillUploadFile {
    pub path: String,
    pub content: String,
}

pub struct SkillAssetService<'a, R: ?Sized> {
    repo: &'a R,
}

impl<'a, R: ?Sized> SkillAssetService<'a, R>
where
    R: SkillAssetRepository,
{
    pub fn new(repo: &'a R) -> Self {
        Self { repo }
    }

    pub async fn list(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<SkillAsset>, SkillAssetApplicationError> {
        Ok(self.repo.list_by_project(project_id).await?)
    }

    pub async fn get(&self, id: Uuid) -> Result<SkillAsset, SkillAssetApplicationError> {
        self.repo.get(id).await?.ok_or_else(|| {
            SkillAssetApplicationError::NotFound(format!("skill_asset 不存在: {id}"))
        })
    }

    pub async fn create(
        &self,
        input: CreateSkillAssetInput,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let key = validate_skill_key(&input.key)?;
        validate_display_name(&input.display_name)?;
        validate_description(&input.description)?;
        self.ensure_key_available(input.project_id, &key, None)
            .await?;

        let files = build_files(Uuid::nil(), input.files)?;
        validate_skill_files(
            &key,
            &input.description,
            input.disable_model_invocation,
            &files,
        )?;

        let mut asset = SkillAsset::new_user(
            input.project_id,
            key,
            input.display_name.trim(),
            input.description.trim(),
            input.disable_model_invocation,
        );
        asset.files = files
            .into_iter()
            .map(|mut file| {
                file.skill_asset_id = asset.id;
                file
            })
            .collect();

        self.repo.create(&asset).await?;
        Ok(asset)
    }

    pub async fn update(
        &self,
        id: Uuid,
        input: UpdateSkillAssetInput,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let mut asset = self.get(id).await?;
        if let Some(key) = input.key {
            let key = validate_skill_key(&key)?;
            if key != asset.key {
                self.ensure_key_available(asset.project_id, &key, Some(asset.id))
                    .await?;
            }
            asset.key = key;
        }
        if let Some(display_name) = input.display_name {
            validate_display_name(&display_name)?;
            asset.display_name = display_name.trim().to_string();
        }
        if let Some(description) = input.description {
            validate_description(&description)?;
            asset.description = description.trim().to_string();
        }
        if let Some(disable_model_invocation) = input.disable_model_invocation {
            asset.disable_model_invocation = disable_model_invocation;
        }
        if let Some(files) = input.files {
            asset.files = build_files(asset.id, files)?;
        }
        validate_skill_files(
            &asset.key,
            &asset.description,
            asset.disable_model_invocation,
            &asset.files,
        )?;
        asset.touch();
        self.repo.update(&asset).await?;
        Ok(asset)
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), SkillAssetApplicationError> {
        self.repo.delete(id).await?;
        Ok(())
    }

    pub async fn bootstrap_builtins(
        &self,
        project_id: Uuid,
        builtin_key: Option<&str>,
    ) -> Result<Vec<SkillAsset>, SkillAssetApplicationError> {
        let templates = match builtin_key.map(str::trim).filter(|value| !value.is_empty()) {
            Some(key) => vec![get_builtin_skill_asset_template(key).ok_or_else(|| {
                SkillAssetApplicationError::NotFound(format!("内嵌 Skill 模板不存在: {key}"))
            })?],
            None => list_builtin_skill_asset_templates(),
        };

        let mut created_or_existing = Vec::with_capacity(templates.len());
        for template in templates {
            if let Some(existing) = self
                .repo
                .get_by_project_and_builtin_key(project_id, template.builtin_key)
                .await?
            {
                created_or_existing.push(existing);
                continue;
            }
            created_or_existing.push(
                self.create_from_builtin_template(project_id, template)
                    .await?,
            );
        }
        Ok(created_or_existing)
    }

    pub async fn reset_from_builtin(
        &self,
        id: Uuid,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let mut asset = self.get(id).await?;
        let builtin_key = asset
            .source
            .builtin_key()
            .map(ToString::to_string)
            .ok_or_else(|| {
                SkillAssetApplicationError::BadRequest(
                    "只有 builtin_seed Skill 可以 reset".to_string(),
                )
            })?;
        let template = get_builtin_skill_asset_template(&builtin_key).ok_or_else(|| {
            SkillAssetApplicationError::NotFound(format!("内嵌 Skill 模板不存在: {builtin_key}"))
        })?;
        let (description, disable_model_invocation, files) =
            files_from_embedded_bundle(asset.id, template.bundle)?;
        asset.key = template.bundle.name.to_string();
        asset.display_name = template.display_name.to_string();
        asset.description = description;
        asset.disable_model_invocation = disable_model_invocation;
        asset.files = files;
        asset.touch();
        self.repo.update(&asset).await?;
        Ok(asset)
    }

    pub async fn import_uploaded_files(
        &self,
        project_id: Uuid,
        files: Vec<RawSkillUploadFile>,
    ) -> Result<Vec<SkillAsset>, SkillAssetApplicationError> {
        let grouped = group_uploaded_skill_files(files)?;
        let mut results = Vec::new();
        for (key, input_files) in grouped {
            let skill_md = input_files
                .iter()
                .find(|file| file.path == "SKILL.md")
                .ok_or_else(|| {
                    SkillAssetApplicationError::BadRequest(format!("Skill `{key}` 缺少 SKILL.md"))
                })?;
            let meta = parse_skill_metadata(&skill_md.content)?;
            let description = meta.description;
            let disable_model_invocation = meta.disable_model_invocation;
            if let Some(mut existing) = self.repo.get_by_project_and_key(project_id, &key).await? {
                existing.display_name = key.clone();
                existing.description = description.clone();
                existing.disable_model_invocation = disable_model_invocation;
                existing.files = build_files(existing.id, input_files)?;
                validate_skill_files(
                    &existing.key,
                    &existing.description,
                    existing.disable_model_invocation,
                    &existing.files,
                )?;
                existing.touch();
                self.repo.update(&existing).await?;
                results.push(existing);
            } else {
                let created = self
                    .create(CreateSkillAssetInput {
                        project_id,
                        key: key.clone(),
                        display_name: key,
                        description,
                        disable_model_invocation,
                        files: input_files,
                    })
                    .await?;
                results.push(created);
            }
        }
        Ok(results)
    }

    async fn create_from_builtin_template(
        &self,
        project_id: Uuid,
        template: BuiltinSkillAssetTemplate,
    ) -> Result<SkillAsset, SkillAssetApplicationError> {
        let (description, disable_model_invocation, files) =
            files_from_embedded_bundle(Uuid::nil(), template.bundle)?;
        self.ensure_key_available(project_id, template.bundle.name, None)
            .await?;
        let mut asset = SkillAsset::new_builtin_seed(
            project_id,
            template.builtin_key,
            template.bundle.name,
            template.display_name,
            description,
            disable_model_invocation,
        );
        asset.files = files
            .into_iter()
            .map(|mut file| {
                file.skill_asset_id = asset.id;
                file
            })
            .collect();
        self.repo.create(&asset).await?;
        Ok(asset)
    }

    async fn ensure_key_available(
        &self,
        project_id: Uuid,
        key: &str,
        allow_id: Option<Uuid>,
    ) -> Result<(), SkillAssetApplicationError> {
        if let Some(existing) = self.repo.get_by_project_and_key(project_id, key).await?
            && Some(existing.id) != allow_id
        {
            return Err(SkillAssetApplicationError::Conflict(format!(
                "skill_asset key 已存在: {key}"
            )));
        }
        Ok(())
    }
}

fn files_from_embedded_bundle(
    asset_id: Uuid,
    bundle: &EmbeddedSkillBundle,
) -> Result<(String, bool, Vec<SkillAssetFile>), SkillAssetApplicationError> {
    bundle
        .validate()
        .map_err(|error| SkillAssetApplicationError::Internal(error.to_string()))?;
    let files = bundle
        .files
        .iter()
        .map(|file| SkillAssetFileInput {
            path: file.relative_path.to_string(),
            content: file.content.to_string(),
        })
        .collect::<Vec<_>>();
    let files = build_files(asset_id, files)?;
    let skill_md = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or_else(|| {
            SkillAssetApplicationError::Internal("内嵌 Skill 缺少 SKILL.md".to_string())
        })?;
    let meta = parse_skill_metadata(&skill_md.content)?;
    validate_skill_files(
        bundle.name,
        &meta.description,
        meta.disable_model_invocation,
        &files,
    )?;
    Ok((meta.description, meta.disable_model_invocation, files))
}

fn build_files(
    asset_id: Uuid,
    files: Vec<SkillAssetFileInput>,
) -> Result<Vec<SkillAssetFile>, SkillAssetApplicationError> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::with_capacity(files.len());
    for file in files {
        let path = normalize_skill_file_path(&file.path)?;
        if !seen.insert(path.clone()) {
            return Err(SkillAssetApplicationError::BadRequest(format!(
                "Skill 文件路径重复: {path}"
            )));
        }
        result.push(SkillAssetFile::new(
            asset_id,
            path.clone(),
            file.content,
            SkillAssetFileKind::from_path(&path),
        ));
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

fn validate_skill_files(
    key: &str,
    description: &str,
    disable_model_invocation: bool,
    files: &[SkillAssetFile],
) -> Result<(), SkillAssetApplicationError> {
    let skill_md = files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .ok_or_else(|| {
            SkillAssetApplicationError::BadRequest("Skill 必须包含 SKILL.md".to_string())
        })?;
    let meta = parse_skill_metadata(&skill_md.content)?;
    if meta.name != key {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "SKILL.md frontmatter name `{}` 必须等于 skill key `{key}`",
            meta.name
        )));
    }
    if meta.description.trim() != description.trim() {
        return Err(SkillAssetApplicationError::BadRequest(
            "SKILL.md frontmatter description 必须与资产描述一致".to_string(),
        ));
    }
    if meta.disable_model_invocation != disable_model_invocation {
        return Err(SkillAssetApplicationError::BadRequest(
            "SKILL.md frontmatter disable-model-invocation 必须与资产设置一致".to_string(),
        ));
    }
    Ok(())
}

struct ParsedSkillMetadata {
    name: String,
    description: String,
    disable_model_invocation: bool,
}

fn parse_skill_metadata(content: &str) -> Result<ParsedSkillMetadata, SkillAssetApplicationError> {
    let (frontmatter, _body) = parse_skill_file(content);
    let frontmatter = frontmatter.ok_or_else(|| {
        SkillAssetApplicationError::BadRequest("SKILL.md 必须包含 YAML frontmatter".to_string())
    })?;
    let name = frontmatter
        .name
        .as_deref()
        .map(validate_skill_key)
        .transpose()?
        .ok_or_else(|| SkillAssetApplicationError::BadRequest("SKILL.md 缺少 name".to_string()))?;
    let description = frontmatter
        .description
        .as_deref()
        .ok_or_else(|| {
            SkillAssetApplicationError::BadRequest("SKILL.md 缺少 description".to_string())
        })?
        .trim()
        .to_string();
    validate_description(&description)?;
    Ok(ParsedSkillMetadata {
        name,
        description,
        disable_model_invocation: frontmatter.disable_model_invocation,
    })
}

fn validate_skill_key(key: &str) -> Result<String, SkillAssetApplicationError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "skill key 不能为空".to_string(),
        ));
    }
    if key.len() > MAX_SKILL_KEY_LENGTH {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "skill key 不能超过 {MAX_SKILL_KEY_LENGTH} 字符"
        )));
    }
    if !key
        .chars()
        .all(|ch| matches!(ch, 'a'..='z' | '0'..='9' | '-'))
    {
        return Err(SkillAssetApplicationError::BadRequest(
            "skill key 只能包含小写字母、数字和连字符".to_string(),
        ));
    }
    Ok(key.to_string())
}

fn validate_display_name(display_name: &str) -> Result<(), SkillAssetApplicationError> {
    let value = display_name.trim();
    if value.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "display_name 不能为空".to_string(),
        ));
    }
    if value.len() > 128 {
        return Err(SkillAssetApplicationError::BadRequest(
            "display_name 不能超过 128 字符".to_string(),
        ));
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<(), SkillAssetApplicationError> {
    let value = description.trim();
    if value.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "description 不能为空".to_string(),
        ));
    }
    if value.len() > MAX_SKILL_DESCRIPTION_LENGTH {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "description 不能超过 {MAX_SKILL_DESCRIPTION_LENGTH} 字符"
        )));
    }
    Ok(())
}

fn normalize_skill_file_path(path: &str) -> Result<String, SkillAssetApplicationError> {
    let normalized = path.trim().replace('\\', "/");
    let normalized = normalized.trim_matches('/').to_string();
    if normalized.is_empty() || normalized.contains(':') {
        return Err(SkillAssetApplicationError::BadRequest(format!(
            "Skill 文件路径非法: {path}"
        )));
    }
    for segment in normalized.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err(SkillAssetApplicationError::BadRequest(format!(
                "Skill 文件路径非法: {path}"
            )));
        }
    }
    Ok(normalized)
}

fn group_uploaded_skill_files(
    files: Vec<RawSkillUploadFile>,
) -> Result<BTreeMap<String, Vec<SkillAssetFileInput>>, SkillAssetApplicationError> {
    if files.is_empty() {
        return Err(SkillAssetApplicationError::BadRequest(
            "上传内容为空".to_string(),
        ));
    }

    let normalized = files
        .into_iter()
        .map(|file| Ok((normalize_skill_file_path(&file.path)?, file.content)))
        .collect::<Result<Vec<_>, SkillAssetApplicationError>>()?;

    let root_skill_count = normalized
        .iter()
        .filter(|(path, _)| path == "SKILL.md")
        .count();
    if root_skill_count > 1 {
        return Err(SkillAssetApplicationError::BadRequest(
            "上传内容包含重复的根 SKILL.md".to_string(),
        ));
    }

    if root_skill_count == 1 {
        let skill_md = normalized
            .iter()
            .find(|(path, _)| path == "SKILL.md")
            .expect("counted root skill");
        let key = parse_skill_metadata(&skill_md.1)?.name;
        return Ok(BTreeMap::from([(
            key,
            normalized
                .into_iter()
                .map(|(path, content)| SkillAssetFileInput { path, content })
                .collect(),
        )]));
    }

    let mut grouped: BTreeMap<String, Vec<SkillAssetFileInput>> = BTreeMap::new();
    for (path, content) in normalized {
        let (key, relative_path) = path.split_once('/').ok_or_else(|| {
            SkillAssetApplicationError::BadRequest(format!(
                "多 Skill 上传时文件必须位于 <skill-key>/... 目录下: {path}"
            ))
        })?;
        let key = validate_skill_key(key)?;
        grouped.entry(key).or_default().push(SkillAssetFileInput {
            path: relative_path.to_string(),
            content,
        });
    }
    for (key, files) in &grouped {
        if !files.iter().any(|file| file.path == "SKILL.md") {
            return Err(SkillAssetApplicationError::BadRequest(format!(
                "Skill `{key}` 缺少 SKILL.md"
            )));
        }
    }
    Ok(grouped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemorySkillAssetRepo {
        assets: Mutex<Vec<SkillAsset>>,
    }

    #[async_trait::async_trait]
    impl SkillAssetRepository for InMemorySkillAssetRepo {
        async fn create(&self, asset: &SkillAsset) -> Result<(), DomainError> {
            self.assets.lock().unwrap().push(asset.clone());
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| asset.id == id)
                .cloned())
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| asset.project_id == project_id && asset.key == key)
                .cloned())
        }

        async fn get_by_project_and_builtin_key(
            &self,
            project_id: Uuid,
            builtin_key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .find(|asset| {
                    asset.project_id == project_id
                        && asset.source.builtin_key() == Some(builtin_key)
                })
                .cloned())
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .iter()
                .filter(|asset| asset.project_id == project_id)
                .cloned()
                .collect())
        }

        async fn update(&self, asset: &SkillAsset) -> Result<(), DomainError> {
            let mut guard = self.assets.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|existing| existing.id == asset.id) {
                *existing = asset.clone();
                Ok(())
            } else {
                Err(DomainError::NotFound {
                    entity: "skill_asset",
                    id: asset.id.to_string(),
                })
            }
        }

        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.assets.lock().unwrap().retain(|asset| asset.id != id);
            Ok(())
        }
    }

    fn skill_file(key: &str, description: &str) -> SkillAssetFileInput {
        SkillAssetFileInput {
            path: "SKILL.md".to_string(),
            content: format!("---\nname: {key}\ndescription: \"{description}\"\n---\n# Body\n"),
        }
    }

    #[tokio::test]
    async fn create_rejects_missing_skill_md_and_mismatched_frontmatter() {
        let repo = InMemorySkillAssetRepo::default();
        let service = SkillAssetService::new(&repo);
        let project_id = Uuid::new_v4();

        let missing = service
            .create(CreateSkillAssetInput {
                project_id,
                key: "writer".to_string(),
                display_name: "Writer".to_string(),
                description: "写作辅助".to_string(),
                disable_model_invocation: false,
                files: vec![SkillAssetFileInput {
                    path: "references/style.md".to_string(),
                    content: "style".to_string(),
                }],
            })
            .await;
        assert!(matches!(
            missing,
            Err(SkillAssetApplicationError::BadRequest(_))
        ));

        let mismatch = service
            .create(CreateSkillAssetInput {
                project_id,
                key: "writer".to_string(),
                display_name: "Writer".to_string(),
                description: "写作辅助".to_string(),
                disable_model_invocation: false,
                files: vec![skill_file("other", "写作辅助")],
            })
            .await;
        assert!(matches!(
            mismatch,
            Err(SkillAssetApplicationError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn builtin_bootstrap_is_idempotent_and_reset_restores_template() {
        let repo = InMemorySkillAssetRepo::default();
        let service = SkillAssetService::new(&repo);
        let project_id = Uuid::new_v4();

        let first = service
            .bootstrap_builtins(project_id, Some("canvas-system"))
            .await
            .expect("bootstrap");
        assert_eq!(first.len(), 1);
        let asset = first[0].clone();

        let edited_description = "用户编辑后的描述";
        let edited = service
            .update(
                asset.id,
                UpdateSkillAssetInput {
                    description: Some(edited_description.to_string()),
                    files: Some(vec![skill_file(&asset.key, edited_description)]),
                    ..Default::default()
                },
            )
            .await
            .expect("edit builtin seed");
        assert_eq!(edited.description, edited_description);

        let second = service
            .bootstrap_builtins(project_id, Some("canvas-system"))
            .await
            .expect("bootstrap again");
        assert_eq!(second[0].description, edited_description);

        let reset = service
            .reset_from_builtin(asset.id)
            .await
            .expect("reset builtin seed");
        assert_ne!(reset.description, edited_description);
        assert!(reset.files.iter().any(|file| file.path == "SKILL.md"));
    }

    #[test]
    fn upload_grouping_accepts_root_skill_and_multi_skill_directory() {
        let root = group_uploaded_skill_files(vec![RawSkillUploadFile {
            path: "SKILL.md".to_string(),
            content: "---\nname: writer\ndescription: \"写作辅助\"\n---\n".to_string(),
        }])
        .expect("root skill");
        assert!(root.contains_key("writer"));

        let grouped = group_uploaded_skill_files(vec![
            RawSkillUploadFile {
                path: "writer/SKILL.md".to_string(),
                content: "---\nname: writer\ndescription: \"写作辅助\"\n---\n".to_string(),
            },
            RawSkillUploadFile {
                path: "research/SKILL.md".to_string(),
                content: "---\nname: research\ndescription: \"调研\"\n---\n".to_string(),
            },
        ])
        .expect("multi skill");
        assert_eq!(grouped.len(), 2);
    }
}
