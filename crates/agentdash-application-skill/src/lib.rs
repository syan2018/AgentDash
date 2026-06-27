pub mod asset {
    pub use crate::skill_asset::{
        CreateSkillAssetInput, ImportRemoteSkillAssetInput, MaterializedSkillTemplate,
        RemoteSkillTemplateInput, SkillAssetFileInput, SkillAssetService, UpdateSkillAssetInput,
        content_from_bytes, materialize_remote_skill_template, prepare_remote_skill_import,
        remote_skill_url_source_ref,
    };
}

pub mod baseline {}

pub mod builtin {
    pub use crate::skill_asset::{
        BuiltinSkillAssetTemplate, get_builtin_skill_asset_template,
        list_builtin_skill_asset_templates,
    };
}

pub mod discovery;

pub mod error {
    pub use crate::skill_asset::SkillAssetApplicationError;
}

pub mod skill;
pub mod skill_asset;
