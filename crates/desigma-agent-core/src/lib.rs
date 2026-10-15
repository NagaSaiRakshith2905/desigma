pub mod engine;
pub mod revision_history;

pub const LIVE_PROTOCOL_VERSION: u32 = 1;
pub const TOOL_CATALOG_VERSION: u32 = 3;

mod contract;
mod verification;

/// Validate a JSON value against the supported public tool-schema subset.
pub fn validate_public_schema(value: &serde_json::Value, schema: &serde_json::Value) -> Result<(), String> {
    contract::validate(value, schema)
}
