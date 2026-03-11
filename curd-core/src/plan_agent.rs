use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanAgentOptions {
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub allow_autonomous_steps: bool,
}
