mod command;
mod definition;
mod error;
mod instance;
mod presentation;
mod repository;
mod source;

pub use command::*;
pub use definition::*;
pub use error::{InteractionError, InteractionResult};
pub use instance::*;
pub use presentation::*;
pub use repository::*;
pub use source::*;

pub const DEFINITION_FORMAT_V1: u16 = 1;
pub const INTERACTION_CONTRACT_V1: u16 = 1;
pub const SOURCE_BUNDLE_FORMAT_V1: u16 = 1;
