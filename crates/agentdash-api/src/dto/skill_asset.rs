use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SkillAssetFileBlobQuery {
    pub path: String,
}
