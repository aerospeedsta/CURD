use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    Owner,
    Editor,
    Planner,
    Reviewer,
    Observer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipantBinding {
    pub participant_id: String,
    pub display_name: String,
    pub role: ParticipantRole,
    pub is_human: bool,
    pub pubkey_hex: Option<String>,
    #[serde(default)]
    pub bound_by: Option<String>,
    #[serde(default)]
    pub binding_origin: String,
    #[serde(default)]
    pub bootstrap: bool,
    pub joined_at_secs: u64,
    pub last_seen_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanOverrideLock {
    pub resource_key: String,
    pub owner_participant_id: String,
    pub acquired_at_secs: u64,
    pub expires_at_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationState {
    #[serde(alias = "session_id")]
    pub collaboration_id: Uuid,
    pub workspace_root: PathBuf,
    pub participants: Vec<ParticipantBinding>,
    pub active_plan_set_id: Option<Uuid>,
    #[serde(default)]
    pub bootstrap_participant_id: Option<String>,
    #[serde(default)]
    pub human_override_locks: Vec<HumanOverrideLock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaborationCapability {
    Read,
    Search,
    Graph,
    Doc,
    PlanCreate,
    PlanSimulate,
    PlanCompare,
    PlanReview,
    VariantPromote,
    WorkspaceMutate,
    WorkspaceCommit,
    RoleBind,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn role_allows(role: ParticipantRole, cap: CollaborationCapability) -> bool {
    match role {
        ParticipantRole::Owner => true,
        ParticipantRole::Editor => cap != CollaborationCapability::RoleBind,
        ParticipantRole::Planner => matches!(
            cap,
            CollaborationCapability::Read
                | CollaborationCapability::Search
                | CollaborationCapability::Graph
                | CollaborationCapability::Doc
                | CollaborationCapability::PlanCreate
                | CollaborationCapability::PlanSimulate
                | CollaborationCapability::PlanCompare
        ),
        ParticipantRole::Reviewer => matches!(
            cap,
            CollaborationCapability::Read
                | CollaborationCapability::Search
                | CollaborationCapability::Graph
                | CollaborationCapability::Doc
                | CollaborationCapability::PlanCompare
                | CollaborationCapability::PlanReview
        ),
        ParticipantRole::Observer => matches!(
            cap,
            CollaborationCapability::Read
                | CollaborationCapability::Search
                | CollaborationCapability::Graph
                | CollaborationCapability::Doc
        ),
    }
}

pub struct CollaborationStore {
    workspace_root: PathBuf,
}

impl CollaborationStore {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    fn dir(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("collaborations")
    }

    fn legacy_dir(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("sessions")
    }

    fn state_path(&self, collaboration_id: Uuid) -> PathBuf {
        self.dir().join(format!("{collaboration_id}.json"))
    }

    fn legacy_state_path(&self, collaboration_id: Uuid) -> PathBuf {
        self.legacy_dir().join(format!("{collaboration_id}.json"))
    }

    pub fn load_or_create(&self, collaboration_id: Uuid) -> Result<CollaborationState> {
        let path = self.state_path(collaboration_id);
        if path.exists() {
            return Ok(serde_json::from_str(&fs::read_to_string(path)?)?);
        }
        let legacy_path = self.legacy_state_path(collaboration_id);
        if legacy_path.exists() {
            let mut state: CollaborationState =
                serde_json::from_str(&fs::read_to_string(legacy_path)?)?;
            state.collaboration_id = collaboration_id;
            return Ok(state);
        }
        Ok(CollaborationState {
            collaboration_id,
            workspace_root: self.workspace_root.clone(),
            participants: Vec::new(),
            active_plan_set_id: None,
            bootstrap_participant_id: None,
            human_override_locks: Vec::new(),
        })
    }

    pub fn save(&self, state: &CollaborationState) -> Result<()> {
        fs::create_dir_all(self.dir())?;
        fs::write(
            self.state_path(state.collaboration_id),
            serde_json::to_string_pretty(state)?,
        )?;
        Ok(())
    }

    pub fn find_participant<'a>(
        &self,
        state: &'a CollaborationState,
        participant_id: &str,
    ) -> Option<&'a ParticipantBinding> {
        state
            .participants
            .iter()
            .find(|p| p.participant_id == participant_id)
    }

    pub fn prune_expired_locks(&self, state: &mut CollaborationState) {
        let now = now_secs();
        state
            .human_override_locks
            .retain(|lock| lock.expires_at_secs > now);
    }

    pub fn bind_participant(
        &self,
        collaboration_id: Uuid,
        participant_id: &str,
        display_name: Option<&str>,
        role: ParticipantRole,
        is_human: bool,
        pubkey_hex: Option<String>,
        bound_by: Option<String>,
        binding_origin: &str,
        bootstrap: bool,
    ) -> Result<CollaborationState> {
        let mut state = self.load_or_create(collaboration_id)?;
        let now = now_secs();
        if let Some(existing) = state
            .participants
            .iter_mut()
            .find(|p| p.participant_id == participant_id)
        {
            existing.display_name = display_name.unwrap_or(participant_id).to_string();
            existing.role = role;
            existing.is_human = is_human;
            existing.pubkey_hex = pubkey_hex;
            existing.bound_by = bound_by;
            existing.binding_origin = binding_origin.to_string();
            existing.bootstrap = bootstrap;
            existing.last_seen_secs = now;
        } else {
            state.participants.push(ParticipantBinding {
                participant_id: participant_id.to_string(),
                display_name: display_name.unwrap_or(participant_id).to_string(),
                role,
                is_human,
                pubkey_hex,
                bound_by,
                binding_origin: binding_origin.to_string(),
                bootstrap,
                joined_at_secs: now,
                last_seen_secs: now,
            });
        }
        if bootstrap && state.bootstrap_participant_id.is_none() {
            state.bootstrap_participant_id = Some(participant_id.to_string());
        }
        self.save(&state)?;
        Ok(state)
    }

    pub fn claim_lock(
        &self,
        collaboration_id: Uuid,
        participant_id: &str,
        resource_key: &str,
        ttl_secs: u64,
    ) -> Result<CollaborationState> {
        let mut state = self.load_or_create(collaboration_id)?;
        let now = now_secs();
        state
            .human_override_locks
            .retain(|lock| lock.expires_at_secs > now && lock.resource_key != resource_key);
        state.human_override_locks.push(HumanOverrideLock {
            resource_key: resource_key.to_string(),
            owner_participant_id: participant_id.to_string(),
            acquired_at_secs: now,
            expires_at_secs: now + ttl_secs,
        });
        self.save(&state)?;
        Ok(state)
    }

    pub fn release_lock(
        &self,
        collaboration_id: Uuid,
        participant_id: &str,
        resource_key: &str,
    ) -> Result<CollaborationState> {
        let mut state = self.load_or_create(collaboration_id)?;
        state.human_override_locks.retain(|lock| {
            !(lock.owner_participant_id == participant_id && lock.resource_key == resource_key)
        });
        self.save(&state)?;
        Ok(state)
    }
}
