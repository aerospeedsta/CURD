use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisclosureLevel {
    L0,
    L1,
    L2,
    L3,
    L4,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScopeSeed {
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExpansionRequest {
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisclosureRequest {
    pub level: DisclosureLevel,
    #[serde(default)]
    pub seed: ScopeSeed,
    #[serde(default)]
    pub expansion: Option<ExpansionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DisclosureBundle {
    pub level: String,
    #[serde(default)]
    pub summary: Value,
    #[serde(default)]
    pub expandable_groups: Vec<String>,
    #[serde(default)]
    pub expandable_nodes: Vec<String>,
}

pub fn build_disclosure_bundle(request: &DisclosureRequest, summary: Value) -> DisclosureBundle {
    DisclosureBundle {
        level: format!("{:?}", request.level).to_ascii_lowercase(),
        summary,
        expandable_groups: request.seed.groups.clone(),
        expandable_nodes: request.seed.node_ids.clone(),
    }
}
