mod entity;
mod value_objects;

pub use entity::{Task, TaskSpecMut};
pub use value_objects::{
    TaskDispatchPreference, Artifact, ArtifactType, TaskExecutionProjection, TaskStatus,
};
