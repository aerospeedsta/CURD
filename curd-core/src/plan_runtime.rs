use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanRuntimeTask {
    #[serde(default)]
    pub task_name: String,
    #[serde(default)]
    pub background: bool,
}
