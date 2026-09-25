use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Tool,
    Expert,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageRef {
    pub kind: PackageKind,
    pub id: String,
    pub version: String,
}

pub const MAX_EXPERT_VIEW_BYTES: usize = 65_536;

#[derive(Clone, Copy)]
pub struct ExpertBudget {
    pub max_view_calls: u32,
    pub max_view_bytes: usize,
    pub max_output_bytes: usize,
    pub max_insights: usize,
    pub max_model_calls: u32,
    pub max_tool_calls: u32,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
}

impl Default for ExpertBudget {
    fn default() -> Self {
        Self {
            max_view_calls: 4,
            max_view_bytes: MAX_EXPERT_VIEW_BYTES,
            max_output_bytes: 16384,
            max_insights: 8,
            max_model_calls: 10,
            max_tool_calls: 9,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
        }
    }
}
