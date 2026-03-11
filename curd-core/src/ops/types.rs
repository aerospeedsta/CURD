use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeCeiling {
    Lite,
    Full,
}

impl RuntimeCeiling {
    pub fn from_env() -> Self {
        match std::env::var("CURD_MODE")
            .unwrap_or_else(|_| "full".to_string())
            .to_ascii_lowercase()
            .as_str()
        {
            "lite" => Self::Lite,
            _ => Self::Full,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lite => "lite",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalOperationKind {
    Lookup,
    Traverse,
    Read,
    Change,
    Session,
    Exec,
    Plan,
    Review,
    Context,
    Other,
}

impl CanonicalOperationKind {
    pub fn capability_prefix(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Traverse => "traverse",
            Self::Read => "read",
            Self::Change => "change",
            Self::Session => "session",
            Self::Exec => "exec",
            Self::Plan => "plan",
            Self::Review => "review",
            Self::Context => "context",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CapabilityAtom(pub String);

impl CapabilityAtom {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperationScope {
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub actor_id: Option<String>,
    #[serde(default)]
    pub disclosure_level: Option<String>,
    #[serde(default)]
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationEnvelope {
    pub tool_name: String,
    pub operation: CanonicalOperationKind,
    pub capability: CapabilityAtom,
    #[serde(default)]
    pub scope: OperationScope,
}
