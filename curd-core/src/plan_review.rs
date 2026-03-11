use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanReviewBundle {
    #[serde(default)]
    pub changed_nodes: Vec<String>,
    #[serde(default)]
    pub changed_edges: Vec<String>,
    #[serde(default)]
    pub policy_signals: Vec<String>,
}
