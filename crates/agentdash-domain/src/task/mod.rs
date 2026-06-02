mod entity;
mod value_objects;

pub use entity::{Task, TaskSpecMut};
pub use value_objects::{
    Artifact, ArtifactType, TaskDispatchPreference, TaskExecutionProjection, TaskStatus,
};
