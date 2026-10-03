//! Domain-independent bounded model values shared by reasoning and local transforms.
mod schema;
mod wire;

pub use schema::{ModelContractError, ModelOutputFormat, ModelSchema, strict_json, validate_json};
pub use wire::*;
