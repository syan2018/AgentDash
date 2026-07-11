use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ProjectInteractionDefinitionsPath {
    pub project_id: String,
}

#[derive(Debug, Deserialize)]
pub struct InteractionDefinitionPath {
    pub definition_id: String,
}
