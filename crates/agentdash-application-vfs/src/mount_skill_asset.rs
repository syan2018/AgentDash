use std::collections::BTreeSet;

use agentdash_domain::common::{Mount, MountCapability, Vfs};
use agentdash_domain::skill_asset::SkillAssetRepository;
use uuid::Uuid;

use super::mount::{
    PROVIDER_LIFECYCLE_VFS, PROVIDER_SKILL_ASSET_FS, SKILL_ASSET_KEYS_METADATA_KEY,
    SKILL_ASSET_PROJECT_ID_METADATA_KEY,
};
use super::provider::{MountError, SearchQuery, SearchResult};
use super::types::{ListOptions, ListResult, ReadResult};

pub fn build_project_skill_asset_management_mount(
    project_id: Uuid,
    skill_asset_keys: &[String],
) -> Mount {
    Mount {
        id: "skill-assets".to_string(),
        provider: PROVIDER_SKILL_ASSET_FS.to_string(),
        backend_id: String::new(),
        root_ref: format!("skill-assets://project/{project_id}"),
        capabilities: vec![
            MountCapability::Read,
            MountCapability::Write,
            MountCapability::List,
            MountCapability::Search,
        ],
        default_write: true,
        display_name: "Project Skill Assets".to_string(),
        metadata: serde_json::json!({
            SKILL_ASSET_PROJECT_ID_METADATA_KEY: project_id.to_string(),
            SKILL_ASSET_KEYS_METADATA_KEY: normalized_skill_asset_keys(skill_asset_keys),
        }),
    }
}

pub fn append_lifecycle_skill_asset_projection(
    vfs: &mut Vfs,
    project_id: Uuid,
    skill_asset_keys: &[String],
) -> bool {
    let new_keys = normalized_skill_asset_keys(skill_asset_keys);
    if new_keys.is_empty() {
        return true;
    }

    if let Some(lifecycle) = vfs
        .mounts
        .iter_mut()
        .find(|mount| mount.id == "lifecycle" && mount.provider == PROVIDER_LIFECYCLE_VFS)
    {
        let mut metadata = match std::mem::take(&mut lifecycle.metadata) {
            serde_json::Value::Object(object) => object,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut object = serde_json::Map::new();
                object.insert("raw_metadata".to_string(), other);
                object
            }
        };
        let existing_project_matches = metadata
            .get(SKILL_ASSET_PROJECT_ID_METADATA_KEY)
            .and_then(serde_json::Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            == Some(project_id);
        let mut keys = if existing_project_matches {
            metadata
                .get(SKILL_ASSET_KEYS_METADATA_KEY)
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        keys.extend(new_keys);
        let keys = normalized_skill_asset_keys(&keys);
        metadata.insert(
            SKILL_ASSET_PROJECT_ID_METADATA_KEY.to_string(),
            serde_json::Value::String(project_id.to_string()),
        );
        metadata.insert(
            SKILL_ASSET_KEYS_METADATA_KEY.to_string(),
            serde_json::Value::Array(
                keys.iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        lifecycle.metadata = serde_json::Value::Object(metadata);
        return true;
    }

    false
}

pub fn refresh_lifecycle_skill_asset_projection(
    vfs: &mut Vfs,
    project_id: Uuid,
    skill_asset_keys: &[String],
) -> bool {
    let keys = normalized_skill_asset_keys(skill_asset_keys);
    if let Some(lifecycle) = vfs
        .mounts
        .iter_mut()
        .find(|mount| mount.id == "lifecycle" && mount.provider == PROVIDER_LIFECYCLE_VFS)
    {
        let mut metadata = match std::mem::take(&mut lifecycle.metadata) {
            serde_json::Value::Object(object) => object,
            serde_json::Value::Null => serde_json::Map::new(),
            other => {
                let mut object = serde_json::Map::new();
                object.insert("raw_metadata".to_string(), other);
                object
            }
        };
        if keys.is_empty() {
            metadata.remove(SKILL_ASSET_PROJECT_ID_METADATA_KEY);
            metadata.remove(SKILL_ASSET_KEYS_METADATA_KEY);
        } else {
            metadata.insert(
                SKILL_ASSET_PROJECT_ID_METADATA_KEY.to_string(),
                serde_json::Value::String(project_id.to_string()),
            );
            metadata.insert(
                SKILL_ASSET_KEYS_METADATA_KEY.to_string(),
                serde_json::Value::Array(
                    keys.iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
        lifecycle.metadata = serde_json::Value::Object(metadata);
        return true;
    }

    false
}

pub fn lifecycle_mount_has_skill_asset_projection(lifecycle_mount: &Mount) -> bool {
    super::provider_skill_asset::parse_skill_asset_mount_metadata(lifecycle_mount)
        .map(|(_, keys)| !keys.is_empty())
        .unwrap_or(false)
}

pub async fn read_lifecycle_skill_asset_projection(
    repo: &dyn SkillAssetRepository,
    lifecycle_mount: &Mount,
    path: &str,
) -> Result<ReadResult, MountError> {
    super::provider_skill_asset::read_projected_skill_file(repo, lifecycle_mount, path).await
}

pub async fn list_lifecycle_skill_asset_projection(
    repo: &dyn SkillAssetRepository,
    lifecycle_mount: &Mount,
    options: &ListOptions,
) -> Result<ListResult, MountError> {
    super::provider_skill_asset::list_projected_skill_files(repo, lifecycle_mount, options).await
}

pub async fn search_lifecycle_skill_asset_projection(
    repo: &dyn SkillAssetRepository,
    lifecycle_mount: &Mount,
    query: &SearchQuery,
) -> Result<SearchResult, MountError> {
    super::provider_skill_asset::search_projected_skill_files(repo, lifecycle_mount, query).await
}

fn normalized_skill_asset_keys(skill_asset_keys: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    skill_asset_keys
        .iter()
        .map(|key| key.trim())
        .filter(|key| !key.is_empty())
        .filter_map(|key| {
            if seen.insert(key.to_string()) {
                Some(key.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::DomainError;
    use agentdash_domain::skill_asset::{
        SkillAsset, SkillAssetFile, SkillAssetFileKind, SkillAssetRepository,
    };

    struct StaticSkillRepo {
        asset: SkillAsset,
    }

    #[async_trait::async_trait]
    impl SkillAssetRepository for StaticSkillRepo {
        async fn create(&self, _asset: &SkillAsset) -> Result<(), DomainError> {
            Ok(())
        }

        async fn get(&self, id: Uuid) -> Result<Option<SkillAsset>, DomainError> {
            Ok((self.asset.id == id).then(|| self.asset.clone()))
        }

        async fn get_by_project_and_key(
            &self,
            project_id: Uuid,
            key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(
                (self.asset.project_id == project_id && self.asset.key == key)
                    .then(|| self.asset.clone()),
            )
        }

        async fn get_by_project_and_builtin_key(
            &self,
            _project_id: Uuid,
            _builtin_key: &str,
        ) -> Result<Option<SkillAsset>, DomainError> {
            Ok(None)
        }

        async fn list_by_project(&self, project_id: Uuid) -> Result<Vec<SkillAsset>, DomainError> {
            Ok((self.asset.project_id == project_id)
                .then(|| vec![self.asset.clone()])
                .unwrap_or_default())
        }

        async fn update(&self, _asset: &SkillAsset) -> Result<(), DomainError> {
            Ok(())
        }

        async fn delete(&self, _id: Uuid) -> Result<(), DomainError> {
            Ok(())
        }
    }

    fn lifecycle_vfs(project_id: Uuid, keys: &[&str]) -> Vfs {
        Vfs {
            mounts: vec![Mount {
                id: "lifecycle".to_string(),
                provider: PROVIDER_LIFECYCLE_VFS.to_string(),
                backend_id: String::new(),
                root_ref: "lifecycle://run/test".to_string(),
                capabilities: vec![MountCapability::Read, MountCapability::List],
                default_write: false,
                display_name: "Lifecycle".to_string(),
                metadata: serde_json::json!({
                    SKILL_ASSET_PROJECT_ID_METADATA_KEY: project_id.to_string(),
                    SKILL_ASSET_KEYS_METADATA_KEY: keys,
                }),
            }],
            default_mount_id: None,
            source_project_id: None,
            source_story_id: None,
            links: Vec::new(),
        }
    }

    #[test]
    fn lifecycle_skill_projection_appends_for_same_project() {
        let project_id = Uuid::new_v4();
        let mut vfs = lifecycle_vfs(project_id, &["companion-system"]);

        assert!(append_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &["workspace-module-system".to_string()],
        ));

        let keys = vfs.mounts[0]
            .metadata
            .get(SKILL_ASSET_KEYS_METADATA_KEY)
            .and_then(serde_json::Value::as_array)
            .expect("skill keys")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["companion-system", "workspace-module-system"]);
    }

    #[test]
    fn lifecycle_skill_projection_replaces_keys_for_different_project() {
        let mut vfs = lifecycle_vfs(Uuid::new_v4(), &["foreign-project-skill"]);
        let project_id = Uuid::new_v4();

        assert!(append_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &["companion-system".to_string()],
        ));

        assert_eq!(
            vfs.mounts[0]
                .metadata
                .get(SKILL_ASSET_PROJECT_ID_METADATA_KEY)
                .and_then(serde_json::Value::as_str),
            Some(project_id.to_string().as_str())
        );
        let keys = vfs.mounts[0]
            .metadata
            .get(SKILL_ASSET_KEYS_METADATA_KEY)
            .and_then(serde_json::Value::as_array)
            .expect("skill keys")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["companion-system"]);
    }

    #[test]
    fn lifecycle_skill_projection_refresh_replaces_same_project_keys() {
        let project_id = Uuid::new_v4();
        let mut vfs = lifecycle_vfs(project_id, &["stale-skill"]);

        assert!(refresh_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &[
                "companion-system".to_string(),
                "workspace-module-system".to_string(),
            ],
        ));

        let keys = vfs.mounts[0]
            .metadata
            .get(SKILL_ASSET_KEYS_METADATA_KEY)
            .and_then(serde_json::Value::as_array)
            .expect("skill keys")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["companion-system", "workspace-module-system"]);
    }

    #[test]
    fn lifecycle_skill_projection_refresh_clears_empty_keys() {
        let project_id = Uuid::new_v4();
        let mut vfs = lifecycle_vfs(project_id, &["stale-skill"]);

        assert!(refresh_lifecycle_skill_asset_projection(
            &mut vfs,
            project_id,
            &[],
        ));

        assert!(
            vfs.mounts[0]
                .metadata
                .get(SKILL_ASSET_PROJECT_ID_METADATA_KEY)
                .is_none()
        );
        assert!(
            vfs.mounts[0]
                .metadata
                .get(SKILL_ASSET_KEYS_METADATA_KEY)
                .is_none()
        );
    }

    #[test]
    fn lifecycle_skill_projection_detects_projected_keys() {
        let project_id = Uuid::new_v4();
        let vfs = lifecycle_vfs(project_id, &["writer"]);

        assert!(lifecycle_mount_has_skill_asset_projection(&vfs.mounts[0]));
    }

    #[tokio::test]
    async fn lifecycle_skill_asset_projection_helper_reads_projected_skill() {
        let project_id = Uuid::new_v4();
        let mut asset = SkillAsset::new_user(project_id, "writer", "Writer", "写作辅助", false);
        asset.files = vec![SkillAssetFile::new(
            asset.id,
            "SKILL.md",
            "---\nname: writer\ndescription: 写作辅助\n---\n# Writer\n",
            SkillAssetFileKind::Skill,
        )];
        let vfs = lifecycle_vfs(project_id, &["writer"]);
        let repo = StaticSkillRepo { asset };

        let read =
            read_lifecycle_skill_asset_projection(&repo, &vfs.mounts[0], "skills/writer/SKILL.md")
                .await
                .expect("read lifecycle skill projection");
        assert!(read.content.contains("# Writer"));

        let listed = list_lifecycle_skill_asset_projection(
            &repo,
            &vfs.mounts[0],
            &ListOptions {
                path: "skills".to_string(),
                pattern: Some("**/*".to_string()),
                recursive: true,
            },
        )
        .await
        .expect("list lifecycle skill projection");
        assert!(
            listed
                .entries
                .iter()
                .any(|entry| entry.path == "skills/writer/SKILL.md")
        );
    }
}
