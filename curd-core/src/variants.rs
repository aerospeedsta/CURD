use crate::collab::CollaborationStore;
use crate::graph::{EdgeMetadata, GraphEngine};
use crate::{CurdConfig, EngineContext, Plan, dispatch_tool};
use anyhow::Result;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantWorkspaceBackend {
    Shadow,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanVariantStatus {
    Draft,
    Simulated,
    Reviewed,
    Approved,
    Rejected,
    Promoted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSet {
    pub id: Uuid,
    pub session_id: Uuid,
    pub title: String,
    pub objective: String,
    pub created_by: String,
    pub created_at_secs: u64,
    pub baseline_snapshot_id: String,
    pub variants: Vec<Uuid>,
    pub active_variant_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VariantArtifacts {
    pub compare_path: Option<PathBuf>,
    pub graph_path: Option<PathBuf>,
    pub review_path: Option<PathBuf>,
    pub trace_path: Option<PathBuf>,
    pub workspace_locator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantReview {
    pub reviewed_by: String,
    pub decision: String,
    pub summary: Option<String>,
    pub ts_secs: u64,
    #[serde(default)]
    pub graph_risk: Option<Value>,
    #[serde(default)]
    pub impacted_scopes: Vec<Value>,
    #[serde(default)]
    pub comparison_context: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanVariant {
    pub id: Uuid,
    pub plan_set_id: Uuid,
    pub title: String,
    pub strategy_summary: String,
    pub created_by: String,
    pub status: PlanVariantStatus,
    pub backend: VariantWorkspaceBackend,
    pub assumptions: Vec<String>,
    pub risk_tags: Vec<String>,
    pub baseline_snapshot_id: String,
    pub simulated_snapshot_id: Option<String>,
    pub plan: Plan,
    #[serde(default)]
    pub artifacts: VariantArtifacts,
    #[serde(default)]
    pub reviews: Vec<VariantReview>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hash_file(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Some(format!("{:x}", hasher.finalize()))
}

fn snapshot_id(root: &Path) -> String {
    let mut entries = BTreeMap::new();
    let mut builder = WalkBuilder::new(root);
    builder.hidden(false).git_ignore(true).ignore(true).parents(false);
    builder.filter_entry(|entry| {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".git" || name == ".curd" {
                return false;
            }
        }
        true
    });
    for entry in builder.build().flatten() {
        let path = entry.path();
        if path.is_file()
            && let Ok(rel) = path.strip_prefix(root)
            && let Some(hash) = hash_file(path)
        {
            entries.insert(rel.to_string_lossy().to_string(), hash);
        }
    }
    let payload = serde_json::to_vec(&entries).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn copy_workspace(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst)?;
    }
    fs::create_dir_all(dst)?;
    let mut builder = WalkBuilder::new(src);
    builder.hidden(false).git_ignore(true).ignore(true).parents(false);
    builder.filter_entry(|entry| {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".git" || name == ".curd" {
                return false;
            }
        }
        true
    });
    for entry in builder.build().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let rel = path.strip_prefix(src)?;
        let target = dst.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, target)?;
    }
    Ok(())
}

fn estimate_workspace_size(root: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut builder = WalkBuilder::new(root);
    builder.hidden(false).git_ignore(true).ignore(true).parents(false);
    builder.filter_entry(|entry| {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".git" || name == ".curd" {
                return false;
            }
        }
        true
    });
    for entry in builder.build().flatten() {
        if entry.path().is_file() {
            total = total.saturating_add(entry.metadata().map(|m| m.len()).unwrap_or(0));
        }
    }
    Ok(total)
}

pub struct VariantStore {
    workspace_root: PathBuf,
}

impl VariantStore {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: workspace_root.as_ref().to_path_buf(),
        }
    }

    fn plansets_dir(&self) -> PathBuf {
        self.workspace_root.join(".curd").join("plansets")
    }

    fn variants_dir(&self, plan_set_id: Uuid) -> PathBuf {
        self.workspace_root
            .join(".curd")
            .join("variants")
            .join(plan_set_id.to_string())
    }

    fn planset_path(&self, plan_set_id: Uuid) -> PathBuf {
        self.plansets_dir().join(format!("{plan_set_id}.json"))
    }

    fn variant_dir(&self, plan_set_id: Uuid, variant_id: Uuid) -> PathBuf {
        self.variants_dir(plan_set_id).join(variant_id.to_string())
    }

    fn variant_meta_path(&self, plan_set_id: Uuid, variant_id: Uuid) -> PathBuf {
        self.variant_dir(plan_set_id, variant_id).join("meta.json")
    }

    fn variant_plan_path(&self, plan_set_id: Uuid, variant_id: Uuid) -> PathBuf {
        self.variant_dir(plan_set_id, variant_id).join("plan.json")
    }

    fn variant_workspace_path(&self, plan_set_id: Uuid, variant_id: Uuid) -> PathBuf {
        self.variant_dir(plan_set_id, variant_id).join("workspace")
    }

    fn remove_plan_set_artifacts(&self, plan_set_id: Uuid) -> Result<()> {
        let planset_path = self.planset_path(plan_set_id);
        if planset_path.exists() {
            fs::remove_file(planset_path)?;
        }
        let variants_dir = self.variants_dir(plan_set_id);
        if variants_dir.exists() {
            fs::remove_dir_all(variants_dir)?;
        }
        Ok(())
    }

    fn prune_plan_sets(&self, config: &CurdConfig) -> Result<Vec<Uuid>> {
        let retain = config.variants.retain_plan_sets.max(1);
        let mut existing = self.list_plan_sets()?;
        if existing.len() <= retain {
            return Ok(Vec::new());
        }
        existing.sort_by(|a, b| {
            a.created_at_secs
                .cmp(&b.created_at_secs)
                .then_with(|| a.id.cmp(&b.id))
        });
        let to_remove = existing.len().saturating_sub(retain);
        let mut removed = Vec::with_capacity(to_remove);
        for plan_set in existing.into_iter().take(to_remove) {
            self.remove_plan_set_artifacts(plan_set.id)?;
            removed.push(plan_set.id);
        }
        Ok(removed)
    }

    fn prune_variant_workspaces(&self, config: &CurdConfig) -> Result<Vec<(Uuid, Uuid)>> {
        let retain = config.variants.retain_variant_workspaces.max(1);
        let mut simulated = Vec::new();
        for plan_set in self.list_plan_sets()? {
            for variant in self.list_variants(plan_set.id)? {
                let workspace = self.variant_workspace_path(plan_set.id, variant.id);
                if workspace.exists() {
                    let modified = fs::metadata(&workspace)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|ts| ts.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    simulated.push((
                        variant.id,
                        plan_set.id,
                        modified,
                        variant.artifacts.workspace_locator.clone(),
                        variant.simulated_snapshot_id.clone(),
                    ));
                }
            }
        }
        if simulated.len() <= retain {
            return Ok(Vec::new());
        }
        simulated.sort_by(|a, b| {
            a.2.cmp(&b.2)
                .then_with(|| {
                    a.4.clone()
                        .unwrap_or_default()
                        .cmp(&b.4.clone().unwrap_or_default())
                })
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        let to_remove = simulated.len().saturating_sub(retain);
        let mut removed = Vec::with_capacity(to_remove);
        for (variant_id, plan_set_id, _, _, _) in simulated.into_iter().take(to_remove) {
            let workspace = self.variant_workspace_path(plan_set_id, variant_id);
            if workspace.exists() {
                fs::remove_dir_all(&workspace)?;
            }
            let mut variant = self.load_variant(plan_set_id, variant_id)?;
            variant.artifacts.workspace_locator = None;
            variant.simulated_snapshot_id = None;
            if matches!(variant.status, PlanVariantStatus::Simulated) {
                variant.status = PlanVariantStatus::Draft;
            }
            self.save_variant(&variant)?;
            removed.push((plan_set_id, variant_id));
        }
        Ok(removed)
    }

    pub fn create_plan_set(
        &self,
        config: &CurdConfig,
        session_id: Uuid,
        title: String,
        objective: String,
        created_by: String,
    ) -> Result<PlanSet> {
        let existing = self.list_plan_sets()?;
        if existing.len() >= config.variants.max_plan_sets {
            anyhow::bail!(
                "max_plan_sets limit reached ({})",
                config.variants.max_plan_sets
            );
        }
        let plan_set = PlanSet {
            id: Uuid::new_v4(),
            session_id,
            title,
            objective,
            created_by,
            created_at_secs: now_secs(),
            baseline_snapshot_id: snapshot_id(&self.workspace_root),
            variants: Vec::new(),
            active_variant_id: None,
        };
        fs::create_dir_all(self.plansets_dir())?;
        fs::write(
            self.planset_path(plan_set.id),
            serde_json::to_string_pretty(&plan_set)?,
        )?;
        let _ = self.prune_plan_sets(config)?;
        let collab = CollaborationStore::new(&self.workspace_root);
        let mut session_state = collab.load_or_create(session_id)?;
        session_state.active_plan_set_id = Some(plan_set.id);
        collab.save(&session_state)?;
        Ok(plan_set)
    }

    pub fn list_plan_sets(&self) -> Result<Vec<PlanSet>> {
        let mut sets = Vec::new();
        let dir = self.plansets_dir();
        if !dir.exists() {
            return Ok(sets);
        }
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                sets.push(serde_json::from_str(&fs::read_to_string(path)?)?);
            }
        }
        sets.sort_by_key(|s| s.created_at_secs);
        Ok(sets)
    }

    pub fn load_plan_set(&self, plan_set_id: Uuid) -> Result<PlanSet> {
        Ok(serde_json::from_str(&fs::read_to_string(self.planset_path(plan_set_id))?)?)
    }

    fn save_plan_set(&self, plan_set: &PlanSet) -> Result<()> {
        fs::create_dir_all(self.plansets_dir())?;
        fs::write(
            self.planset_path(plan_set.id),
            serde_json::to_string_pretty(plan_set)?,
        )?;
        Ok(())
    }

    pub fn create_variant(
        &self,
        config: &CurdConfig,
        plan_set_id: Uuid,
        title: String,
        strategy_summary: String,
        created_by: String,
        plan: Plan,
        assumptions: Vec<String>,
        risk_tags: Vec<String>,
        backend: VariantWorkspaceBackend,
    ) -> Result<PlanVariant> {
        let plan_set = self.load_plan_set(plan_set_id)?;
        if plan_set.variants.len() >= config.variants.max_variants_per_plan_set {
            anyhow::bail!(
                "max_variants_per_plan_set limit reached ({})",
                config.variants.max_variants_per_plan_set
            );
        }
        let variant = PlanVariant {
            id: Uuid::new_v4(),
            plan_set_id,
            title,
            strategy_summary,
            created_by,
            status: PlanVariantStatus::Draft,
            backend,
            assumptions,
            risk_tags,
            baseline_snapshot_id: plan_set.baseline_snapshot_id.clone(),
            simulated_snapshot_id: None,
            plan,
            artifacts: VariantArtifacts::default(),
            reviews: Vec::new(),
        };
        fs::create_dir_all(self.variant_dir(plan_set_id, variant.id))?;
        fs::write(
            self.variant_meta_path(plan_set_id, variant.id),
            serde_json::to_string_pretty(&variant)?,
        )?;
        fs::write(
            self.variant_plan_path(plan_set_id, variant.id),
            serde_json::to_string_pretty(&variant.plan)?,
        )?;
        let mut updated = plan_set;
        updated.variants.push(variant.id);
        self.save_plan_set(&updated)?;
        Ok(variant)
    }

    pub fn load_variant(&self, plan_set_id: Uuid, variant_id: Uuid) -> Result<PlanVariant> {
        Ok(serde_json::from_str(&fs::read_to_string(
            self.variant_meta_path(plan_set_id, variant_id),
        )?)?)
    }

    fn save_variant(&self, variant: &PlanVariant) -> Result<()> {
        fs::create_dir_all(self.variant_dir(variant.plan_set_id, variant.id))?;
        fs::write(
            self.variant_meta_path(variant.plan_set_id, variant.id),
            serde_json::to_string_pretty(variant)?,
        )?;
        fs::write(
            self.variant_plan_path(variant.plan_set_id, variant.id),
            serde_json::to_string_pretty(&variant.plan)?,
        )?;
        Ok(())
    }

    pub fn list_variants(&self, plan_set_id: Uuid) -> Result<Vec<PlanVariant>> {
        let mut out = Vec::new();
        let dir = self.variants_dir(plan_set_id);
        if !dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                let meta = entry.path().join("meta.json");
                if meta.exists() {
                    out.push(serde_json::from_str(&fs::read_to_string(meta)?)?);
                }
            }
        }
        Ok(out)
    }

    pub async fn simulate_variant(
        &self,
        variant: &mut PlanVariant,
        config: &CurdConfig,
    ) -> Result<Value> {
        match variant.backend {
            VariantWorkspaceBackend::Shadow => {}
            VariantWorkspaceBackend::Worktree => {
                if !config.variants.allow_worktree_backend {
                    anyhow::bail!("worktree backend is disabled in settings.toml");
                }
                anyhow::bail!("worktree backend is not implemented yet");
            }
        }
        let materialized_root = self.variant_workspace_path(variant.plan_set_id, variant.id);
        copy_workspace(&self.workspace_root, &materialized_root)?;
        let materialized_bytes = estimate_workspace_size(&materialized_root)?;
        if materialized_bytes > config.variants.max_materialized_bytes {
            let _ = fs::remove_dir_all(&materialized_root);
            anyhow::bail!(
                "variant materialization exceeded max_materialized_bytes ({})",
                config.variants.max_materialized_bytes
            );
        }

        let ctx = EngineContext::new(materialized_root.to_string_lossy().as_ref());
        let register = dispatch_tool("register_plan", &serde_json::to_value(&variant.plan)?, ctx.as_ref()).await;
        if register.get("error").is_some() {
            return Ok(register);
        }
        let execute = dispatch_tool("execute_active_plan", &json!({}), ctx.as_ref()).await;
        let compare_path = self.variant_dir(variant.plan_set_id, variant.id).join("compare.json");
        let graph_path = self.variant_dir(variant.plan_set_id, variant.id).join("graph.json");
        let workspace_locator = materialized_root.to_string_lossy().to_string();
        let graph = graph_snapshot(&materialized_root)?;
        fs::write(&graph_path, serde_json::to_string_pretty(&graph.as_value())?)?;
        variant.status = PlanVariantStatus::Simulated;
        variant.simulated_snapshot_id = Some(snapshot_id(&materialized_root));
        variant.artifacts.workspace_locator = Some(workspace_locator.clone());
        variant.artifacts.compare_path = Some(compare_path);
        variant.artifacts.graph_path = Some(graph_path);
        self.save_variant(variant)?;
        let _ = self.prune_variant_workspaces(config)?;
        Ok(json!({
            "status": "ok",
            "variant_id": variant.id,
            "workspace_root": workspace_locator,
            "materialized_bytes": materialized_bytes,
            "result": execute
        }))
    }

    pub fn enforce_retention(&self, config: &CurdConfig) -> Result<Value> {
        let pruned_plan_sets = self.prune_plan_sets(config)?;
        let pruned_workspaces = self.prune_variant_workspaces(config)?;
        Ok(json!({
            "status": "ok",
            "pruned_plan_sets": pruned_plan_sets,
            "pruned_variant_workspaces": pruned_workspaces.iter().map(|(plan_set_id, variant_id)| {
                json!({
                    "plan_set_id": plan_set_id,
                    "variant_id": variant_id,
                })
            }).collect::<Vec<_>>(),
        }))
    }

    pub fn compare_variants(
        &self,
        config: &CurdConfig,
        plan_set_id: Uuid,
        variant_ids: &[Uuid],
    ) -> Result<Value> {
        let mut variants = Vec::new();
        let mut snapshots: Vec<(Uuid, String, GraphSnapshot)> = Vec::new();
        for variant_id in variant_ids {
            let mut variant = self.load_variant(plan_set_id, *variant_id)?;
            let workspace = self.variant_workspace_path(plan_set_id, *variant_id);
            if !workspace.exists() {
                variants.push(json!({
                    "variant_id": variant_id,
                    "error": "variant has not been simulated"
                }));
                continue;
            }
            let summary = compare_dirs(&self.workspace_root, &workspace, config.variants.max_compare_files)?;
            let compare_path = self.variant_dir(plan_set_id, *variant_id).join("compare.json");
            let graph_path = self.variant_dir(plan_set_id, *variant_id).join("graph.json");
            let graph_snapshot = graph_snapshot(&workspace)?;
            fs::write(&compare_path, serde_json::to_string_pretty(&summary)?)?;
            fs::write(&graph_path, serde_json::to_string_pretty(&graph_snapshot.as_value())?)?;
            variant.artifacts.compare_path = Some(compare_path.clone());
            variant.artifacts.graph_path = Some(graph_path);
            self.save_variant(&variant)?;
            snapshots.push((variant.id, variant.title.clone(), graph_snapshot));
            variants.push(json!({
                "variant_id": variant_id,
                "title": variant.title,
                "status": variant.status,
                "baseline_snapshot_id": variant.baseline_snapshot_id,
                "simulated_snapshot_id": variant.simulated_snapshot_id,
                "summary": summary,
            }));
        }
        Ok(json!({
            "status": "ok",
            "plan_set_id": plan_set_id,
            "variants": variants,
            "variant_deltas": compare_variant_snapshots(&snapshots),
            "comparison_summary": summarize_variant_comparison(&variants, &snapshots),
        }))
    }

    pub fn promote_variant(&self, ctx: &EngineContext, variant: &mut PlanVariant) -> Result<Value> {
        let workspace = self.variant_workspace_path(variant.plan_set_id, variant.id);
        if !workspace.exists() {
            anyhow::bail!("variant has not been simulated");
        }
        let summary = compare_dirs(&self.workspace_root, &workspace, ctx.config.variants.max_compare_files)?;
        {
            let mut shadow = ctx.we.shadow.lock().map_err(|e| anyhow::anyhow!("{e}"))?;
            if !shadow.is_active() {
                shadow.begin()?;
            }
            let changed = summary
                .get("changed_files")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for entry in changed {
                if let Some(rel) = entry.get("path").and_then(Value::as_str) {
                    if entry.get("status").and_then(Value::as_str) == Some("removed") {
                        anyhow::bail!("promotion currently rejects removed files; use review or manual promotion flow");
                    }
                    let content = fs::read_to_string(workspace.join(rel))?;
                    shadow.stage(Path::new(rel), &content)?;
                }
            }
        }
        variant.status = PlanVariantStatus::Promoted;
        self.save_variant(variant)?;
        let mut plan_set = self.load_plan_set(variant.plan_set_id)?;
        plan_set.active_variant_id = Some(variant.id);
        self.save_plan_set(&plan_set)?;
        Ok(json!({
            "status": "ok",
            "variant_id": variant.id,
            "message": "Variant promoted into the active transaction shadow.",
            "promotion_context": extract_review_context(&summary),
        }))
    }

    pub fn review_variant(
        &self,
        plan_set_id: Uuid,
        variant_id: Uuid,
        reviewed_by: String,
        decision: &str,
        summary: Option<String>,
    ) -> Result<PlanVariant> {
        let mut variant = self.load_variant(plan_set_id, variant_id)?;
        let compare_summary = self
            .load_compare_summary(plan_set_id, variant_id)
            .ok()
            .flatten();
        let review_context = compare_summary
            .as_ref()
            .map(extract_review_context)
            .unwrap_or_else(|| json!({}));
        variant.reviews.push(VariantReview {
            reviewed_by,
            decision: decision.to_string(),
            summary,
            ts_secs: now_secs(),
            graph_risk: review_context.get("graph_risk").cloned(),
            impacted_scopes: review_context
                .get("impacted_scopes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            comparison_context: compare_summary,
        });
        variant.status = match decision {
            "review" => PlanVariantStatus::Reviewed,
            "approve" => PlanVariantStatus::Approved,
            "reject" => PlanVariantStatus::Rejected,
            _ => anyhow::bail!("unsupported review decision: {}", decision),
        };
        let review_path = self.variant_dir(plan_set_id, variant_id).join("review.json");
        fs::write(&review_path, serde_json::to_string_pretty(&variant.reviews)?)?;
        variant.artifacts.review_path = Some(review_path);
        self.save_variant(&variant)?;
        Ok(variant)
    }

    fn load_compare_summary(&self, plan_set_id: Uuid, variant_id: Uuid) -> Result<Option<Value>> {
        let compare_path = self.variant_dir(plan_set_id, variant_id).join("compare.json");
        if !compare_path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(compare_path)?)?))
    }
}

fn extract_review_context(summary: &Value) -> Value {
    json!({
        "graph_risk": summary.get("graph_delta").and_then(|v| v.get("risk_summary")).cloned(),
        "impacted_scopes": summary.get("graph_delta").and_then(|v| v.get("top_impacted_scopes")).cloned().unwrap_or_else(|| json!([])),
        "files_changed": summary.get("files_changed").cloned(),
        "modified": summary.get("modified").cloned(),
        "added": summary.get("added").cloned(),
        "removed": summary.get("removed").cloned(),
    })
}

fn compare_dirs(left: &Path, right: &Path, max_compare_files: usize) -> Result<Value> {
    let mut left_map = HashMap::new();
    let mut right_map = HashMap::new();
    populate_file_map(left, left, &mut left_map)?;
    populate_file_map(right, right, &mut right_map)?;
    let mut all = BTreeMap::new();
    for (path, hash) in &left_map {
        all.insert(path.clone(), (Some(hash.clone()), None::<String>));
    }
    for (path, hash) in &right_map {
        all.entry(path.clone())
            .and_modify(|entry| entry.1 = Some(hash.clone()))
            .or_insert((None, Some(hash.clone())));
    }
    let mut changed = Vec::new();
    let mut added = 0;
    let mut removed = 0;
    let mut modified = 0;
    for (path, (lhs, rhs)) in all {
        let status = match (lhs.as_ref(), rhs.as_ref()) {
            (Some(l), Some(r)) if l == r => None,
            (Some(_), Some(_)) => {
                modified += 1;
                Some("modified")
            }
            (None, Some(_)) => {
                added += 1;
                Some("added")
            }
            (Some(_), None) => {
                removed += 1;
                Some("removed")
            }
            (None, None) => None,
        };
        if let Some(status) = status {
            changed.push(json!({ "path": path, "status": status }));
        }
    }
    if changed.len() > max_compare_files {
        anyhow::bail!(
            "variant comparison exceeded max_compare_files ({})",
            max_compare_files
        );
    }
    let graph_delta = compare_graph_snapshots(left, right)?;
    Ok(json!({
        "files_changed": changed.len(),
        "added": added,
        "removed": removed,
        "modified": modified,
        "changed_files": changed,
        "graph_delta": graph_delta,
    }))
}

fn compare_graph_snapshots(left: &Path, right: &Path) -> Result<Value> {
    let baseline = graph_snapshot(left)?;
    let variant = graph_snapshot(right)?;
    Ok(compare_graph_snapshot_values(&baseline, &variant))
}

fn compare_graph_snapshot_values(baseline: &GraphSnapshot, variant: &GraphSnapshot) -> Value {
    let baseline_nodes = to_string_set(&baseline.node_ids);
    let variant_nodes = to_string_set(&variant.node_ids);
    let baseline_edges = to_edge_set(&baseline.typed_edges);
    let variant_edges = to_edge_set(&variant.typed_edges);

    let added_nodes = diff_string_set(&variant_nodes, &baseline_nodes);
    let removed_nodes = diff_string_set(&baseline_nodes, &variant_nodes);
    let added_edges = diff_edge_set(&variant_edges, &baseline_edges);
    let removed_edges = diff_edge_set(&baseline_edges, &variant_edges);
    let changed_edges = diff_edge_metadata(&baseline.edge_details, &variant.edge_details);

    let top_impacted_scopes = top_impacted_scopes(
        added_nodes
            .iter()
            .chain(removed_nodes.iter())
            .map(String::as_str)
            .chain(added_edges.iter().flat_map(|(from, to, _)| [from.as_str(), to.as_str()]))
            .chain(
                removed_edges
                    .iter()
                    .flat_map(|(from, to, _)| [from.as_str(), to.as_str()]),
            )
            .chain(changed_edges.iter().flat_map(|edge| {
                [
                    edge.get("from").and_then(Value::as_str).unwrap_or(""),
                    edge.get("to").and_then(Value::as_str).unwrap_or(""),
                ]
            })),
    );

    json!({
        "baseline": baseline.summary,
        "variant": variant.summary,
        "added_nodes": added_nodes,
        "removed_nodes": removed_nodes,
        "added_edges": added_edges.iter().map(|(from, to, kind)| {
            json!({ "from": from, "to": to, "kind": kind })
        }).collect::<Vec<_>>(),
        "removed_edges": removed_edges.iter().map(|(from, to, kind)| {
            json!({ "from": from, "to": to, "kind": kind })
        }).collect::<Vec<_>>(),
        "changed_edges": changed_edges,
        "risk_summary": summarize_graph_delta_risk(
            &added_nodes,
            &removed_nodes,
            &added_edges,
            &removed_edges,
            &changed_edges,
        ),
        "top_impacted_scopes": top_impacted_scopes,
    })
}

#[derive(Debug)]
struct GraphSnapshot {
    node_ids: Vec<String>,
    typed_edges: Vec<(String, String, String)>,
    edge_details: BTreeMap<String, GraphEdgeSnapshot>,
    summary: Value,
}

#[derive(Debug, Clone)]
struct GraphEdgeSnapshot {
    from: String,
    to: String,
    kind: String,
    tier: String,
    confidence: f64,
    source: String,
    evidence: Vec<String>,
}

impl GraphSnapshot {
    fn as_value(&self) -> Value {
        json!({
            "summary": self.summary,
            "node_ids": self.node_ids,
            "typed_edges": self.typed_edges.iter().map(|(from, to, kind)| {
                json!({ "from": from, "to": to, "kind": kind })
            }).collect::<Vec<_>>(),
            "detailed_edges": self.edge_details.values().map(|edge| {
                json!({
                    "from": edge.from,
                    "to": edge.to,
                    "kind": edge.kind,
                    "tier": edge.tier,
                    "confidence": edge.confidence,
                    "source": edge.source,
                    "evidence": edge.evidence,
                })
            }).collect::<Vec<_>>(),
        })
    }
}

fn graph_snapshot(root: &Path) -> Result<GraphSnapshot> {
    let graph = GraphEngine::new(root).build_dependency_graph()?;
    let mut node_ids: Vec<String> = graph
        .outgoing
        .keys()
        .chain(graph.incoming.keys())
        .cloned()
        .collect();
    node_ids.sort();
    node_ids.dedup();

    let mut typed_edges = graph.edge_kinds.clone();
    typed_edges.sort();
    typed_edges.dedup();
    if node_ids.is_empty() && typed_edges.is_empty() {
        return naive_graph_snapshot(root);
    }
    let node_count = node_ids.len();
    let edge_count = typed_edges.len();
    let edge_details = build_edge_details(&typed_edges, &graph.edge_metadata);
    let scope_children = summarize_scope_children(&edge_details);

    Ok(GraphSnapshot {
        node_ids,
        typed_edges: typed_edges.clone(),
        edge_details,
        summary: json!({
            "origin": graph.origin,
            "node_count": node_count,
            "edge_count": edge_count,
            "by_kind": summarize_edge_kinds(&typed_edges),
            "by_tier": summarize_edge_metadata(&graph.edge_metadata, &typed_edges, |meta| meta.tier.clone()),
            "by_source": summarize_edge_metadata(&graph.edge_metadata, &typed_edges, |meta| {
                meta.source.clone().unwrap_or_else(|| "unknown".to_string())
            }),
            "scope_children": scope_children,
        }),
    })
}

#[derive(Debug, Clone)]
struct TextFunctionDef {
    name: String,
    offset: usize,
}

fn naive_graph_snapshot(root: &Path) -> Result<GraphSnapshot> {
    let mut node_ids = Vec::new();
    let mut typed_edges: Vec<(String, String, String)> = Vec::new();
    let mut builder = WalkBuilder::new(root);
    builder.hidden(false).git_ignore(true).ignore(true).parents(false);
    builder.filter_entry(|entry| {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".git" || name == ".curd" {
                return false;
            }
        }
        true
    });
    for entry in builder.build().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().to_string();
        let defs = extract_text_function_defs(&content);
        if defs.is_empty() {
            continue;
        }
        for def in &defs {
            node_ids.push(format!("{rel}::{}", def.name));
        }
        for (idx, def) in defs.iter().enumerate() {
            let end = defs
                .get(idx + 1)
                .map(|next| next.offset)
                .unwrap_or(content.len());
            let body = &content[def.offset..end];
            for target in &defs {
                if target.name == def.name {
                    continue;
                }
                if body.contains(&format!("{}(", target.name)) {
                    typed_edges.push((
                        format!("{rel}::{}", def.name),
                        format!("{rel}::{}", target.name),
                        "calls".to_string(),
                    ));
                }
            }
        }
    }
    node_ids.sort();
    node_ids.dedup();
    typed_edges.sort();
    typed_edges.dedup();
    let node_count = node_ids.len();
    let edge_count = typed_edges.len();
    let mut by_tier = BTreeMap::new();
    let mut by_source = BTreeMap::new();
    if edge_count > 0 {
        by_tier.insert("heuristic".to_string(), edge_count);
        by_source.insert("variants:text_scan".to_string(), edge_count);
    }
    let edge_details = typed_edges
        .iter()
        .map(|(from, to, kind)| {
            let edge = GraphEdgeSnapshot {
                from: from.clone(),
                to: to.clone(),
                kind: kind.clone(),
                tier: "heuristic".to_string(),
                confidence: 0.55,
                source: "variants:text_scan".to_string(),
                evidence: vec!["variants".to_string(), "text_scan".to_string()],
            };
            (edge_detail_key(from, to, kind), edge)
        })
        .collect();
    let scope_children = summarize_scope_children(&edge_details);
    Ok(GraphSnapshot {
        node_ids,
        typed_edges: typed_edges.clone(),
        edge_details,
        summary: json!({
            "origin": "text_fallback",
            "node_count": node_count,
            "edge_count": edge_count,
            "by_kind": summarize_edge_kinds(&typed_edges),
            "by_tier": by_tier,
            "by_source": by_source,
            "scope_children": scope_children,
        }),
    })
}

fn extract_text_function_defs(content: &str) -> Vec<TextFunctionDef> {
    let mut defs = Vec::new();
    let mut offset = 0usize;
    for line in content.split_inclusive('\n') {
        if let Some(name) = extract_text_function_name(line) {
            defs.push(TextFunctionDef { name, offset });
        }
        offset += line.len();
    }
    defs
}

fn extract_text_function_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let marker = [
        "pub async fn ",
        "pub(crate) async fn ",
        "async fn ",
        "pub(crate) fn ",
        "pub fn ",
        "fn ",
        "def ",
        "function ",
    ]
    .into_iter()
    .find(|marker| trimmed.starts_with(marker))?;
    let rest = &trimmed[marker.len()..];
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn summarize_edge_kinds(edges: &[(String, String, String)]) -> Value {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, _, kind) in edges {
        *counts.entry(kind.clone()).or_default() += 1;
    }
    serde_json::to_value(counts).unwrap_or_else(|_| json!({}))
}

fn summarize_edge_metadata<F>(
    metadata: &HashMap<String, crate::graph::EdgeMetadata>,
    edges: &[(String, String, String)],
    key_fn: F,
) -> Value
where
    F: Fn(&crate::graph::EdgeMetadata) -> String,
{
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (from, to, kind) in edges {
        let key = format!("{from}|{to}|{kind}");
        if let Some(meta) = metadata.get(&key) {
            *counts.entry(key_fn(meta)).or_default() += 1;
        }
    }
    serde_json::to_value(counts).unwrap_or_else(|_| json!({}))
}

fn build_edge_details(
    typed_edges: &[(String, String, String)],
    metadata: &HashMap<String, EdgeMetadata>,
) -> BTreeMap<String, GraphEdgeSnapshot> {
    typed_edges
        .iter()
        .map(|(from, to, kind)| {
            let meta = metadata
                .get(&edge_detail_key(from, to, kind))
                .cloned()
                .unwrap_or(EdgeMetadata {
                    tier: "unknown".to_string(),
                    confidence: 0.0,
                    source: None,
                    evidence: Vec::new(),
                });
            let edge = GraphEdgeSnapshot {
                from: from.clone(),
                to: to.clone(),
                kind: kind.clone(),
                tier: meta.tier,
                confidence: meta.confidence,
                source: meta.source.unwrap_or_else(|| "unknown".to_string()),
                evidence: meta.evidence,
            };
            (edge_detail_key(from, to, kind), edge)
        })
        .collect()
}

fn edge_detail_key(from: &str, to: &str, kind: &str) -> String {
    format!("{from}\n{to}\n{kind}")
}

fn to_string_set(values: &[String]) -> BTreeSet<String> {
    values.iter().cloned().collect()
}

fn to_edge_set(values: &[(String, String, String)]) -> BTreeSet<(String, String, String)> {
    values.iter().cloned().collect()
}

fn diff_string_set(left: &BTreeSet<String>, right: &BTreeSet<String>) -> Vec<String> {
    left.difference(right).cloned().take(64).collect()
}

fn diff_edge_set(
    left: &BTreeSet<(String, String, String)>,
    right: &BTreeSet<(String, String, String)>,
) -> Vec<(String, String, String)> {
    left.difference(right).cloned().take(64).collect()
}

fn diff_edge_metadata(
    baseline: &BTreeMap<String, GraphEdgeSnapshot>,
    variant: &BTreeMap<String, GraphEdgeSnapshot>,
) -> Vec<Value> {
    let mut shifts = Vec::new();
    for (key, before) in baseline {
        let Some(after) = variant.get(key) else {
            continue;
        };
        let source_changed = before.source != after.source;
        let tier_changed = before.tier != after.tier;
        let evidence_changed = before.evidence != after.evidence;
        let confidence_delta = after.confidence - before.confidence;
        if !source_changed && !tier_changed && !evidence_changed && confidence_delta.abs() < 0.0001
        {
            continue;
        }
        shifts.push(json!({
            "from": before.from,
            "to": before.to,
            "kind": before.kind,
            "before": {
                "tier": before.tier,
                "confidence": before.confidence,
                "source": before.source,
                "evidence": before.evidence,
            },
            "after": {
                "tier": after.tier,
                "confidence": after.confidence,
                "source": after.source,
                "evidence": after.evidence,
            },
            "confidence_delta": confidence_delta,
            "source_changed": source_changed,
            "tier_changed": tier_changed,
            "evidence_changed": evidence_changed,
        }));
    }
    shifts.truncate(64);
    shifts
}

fn summarize_graph_delta_risk(
    added_nodes: &[String],
    removed_nodes: &[String],
    added_edges: &[(String, String, String)],
    removed_edges: &[(String, String, String)],
    changed_edges: &[Value],
) -> Value {
    let mut severity = "low";
    let mut reasons = Vec::new();

    if !removed_nodes.is_empty() {
        severity = "high";
        reasons.push(format!("removed_nodes:{}", removed_nodes.len()));
    }
    if !removed_edges.is_empty() {
        severity = "high";
        reasons.push(format!("removed_edges:{}", removed_edges.len()));
    }

    let unstable_edge_changes = changed_edges
        .iter()
        .filter(|edge| {
            edge.get("source_changed").and_then(Value::as_bool) == Some(true)
                || edge.get("tier_changed").and_then(Value::as_bool) == Some(true)
                || edge
                    .get("confidence_delta")
                    .and_then(Value::as_f64)
                    .map(|delta| delta.abs() >= 0.20)
                    .unwrap_or(false)
        })
        .count();
    if unstable_edge_changes > 0 {
        severity = "high";
        reasons.push(format!("unstable_edge_changes:{unstable_edge_changes}"));
    }

    if severity != "high" {
        if !changed_edges.is_empty() {
            severity = "medium";
            reasons.push(format!("changed_edges:{}", changed_edges.len()));
        }
        if added_edges.len() >= 4 || added_nodes.len() >= 4 {
            severity = "medium";
            reasons.push(format!(
                "large_added_surface:nodes={},edges={}",
                added_nodes.len(),
                added_edges.len()
            ));
        }
    }

    if reasons.is_empty() {
        reasons.push("no_material_graph_risk_signals".to_string());
    }

    json!({
        "severity": severity,
        "reasons": reasons,
        "stats": {
            "added_nodes": added_nodes.len(),
            "removed_nodes": removed_nodes.len(),
            "added_edges": added_edges.len(),
            "removed_edges": removed_edges.len(),
            "changed_edges": changed_edges.len(),
        }
    })
}

fn compare_variant_snapshots(snapshots: &[(Uuid, String, GraphSnapshot)]) -> Vec<Value> {
    let mut deltas = Vec::new();
    for idx in 0..snapshots.len() {
        for other in (idx + 1)..snapshots.len() {
            let (left_id, left_title, left_graph) = &snapshots[idx];
            let (right_id, right_title, right_graph) = &snapshots[other];
            let delta = compare_graph_snapshot_values(left_graph, right_graph);
            deltas.push(json!({
                "left_variant_id": left_id,
                "left_title": left_title,
                "right_variant_id": right_id,
                "right_title": right_title,
                "delta": delta,
            }));
        }
    }
    deltas
}

fn summarize_variant_comparison(variants: &[Value], snapshots: &[(Uuid, String, GraphSnapshot)]) -> Value {
    let variant_summaries: Vec<Value> = variants
        .iter()
        .filter_map(|variant| {
            let variant_id = variant.get("variant_id")?.clone();
            let title = variant.get("title")?.clone();
            let summary = variant.get("summary")?;
            let risk = summary.get("graph_delta")?.get("risk_summary")?.clone();
            let files_changed = summary.get("files_changed").cloned().unwrap_or(Value::Null);
            let top_impacted_scopes = summary
                .get("graph_delta")
                .and_then(|delta| delta.get("top_impacted_scopes"))
                .cloned()
                .unwrap_or_else(|| json!([]));
            Some(json!({
                "variant_id": variant_id,
                "title": title,
                "files_changed": files_changed,
                "risk": risk,
                "top_impacted_scopes": top_impacted_scopes,
            }))
        })
        .collect();

    let mut scope_counts: BTreeMap<String, usize> = BTreeMap::new();
    for variant in variants {
        let mut seen = BTreeSet::new();
        for scope in variant
            .get("summary")
            .and_then(|summary| summary.get("graph_delta"))
            .and_then(|delta| delta.get("top_impacted_scopes"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.get("scope").and_then(Value::as_str))
        {
            seen.insert(scope.to_string());
        }
        for scope in seen {
            *scope_counts.entry(scope).or_default() += 1;
        }
    }
    let common_scopes: Vec<Value> = scope_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(scope, variants)| json!({ "scope": scope, "variants": variants }))
        .collect();

    let mut rankings: Vec<Value> = variant_summaries
        .iter()
        .map(|variant| {
            let risk = variant.get("risk").cloned().unwrap_or(Value::Null);
            let severity = risk
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("low");
            let severity_score = match severity {
                "high" => 3,
                "medium" => 2,
                _ => 1,
            };
            let files_changed = variant
                .get("files_changed")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            json!({
                "variant_id": variant.get("variant_id").cloned().unwrap_or(Value::Null),
                "title": variant.get("title").cloned().unwrap_or(Value::Null),
                "severity": severity,
                "severity_score": severity_score,
                "files_changed": files_changed,
            })
        })
        .collect();
    rankings.sort_by(|a, b| {
        b.get("severity_score")
            .and_then(Value::as_i64)
            .cmp(&a.get("severity_score").and_then(Value::as_i64))
            .then_with(|| {
                b.get("files_changed")
                    .and_then(Value::as_u64)
                    .cmp(&a.get("files_changed").and_then(Value::as_u64))
            })
    });

    json!({
        "variant_count": snapshots.len(),
        "rankings": rankings,
        "common_impacted_scopes": common_scopes,
        "variants": variant_summaries,
    })
}

fn summarize_scope_children(edge_details: &BTreeMap<String, GraphEdgeSnapshot>) -> Vec<Value> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for edge in edge_details.values() {
        if edge.kind != "contains" {
            continue;
        }
        *counts.entry(symbol_scope(&edge.from)).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(scope, direct_children)| json!({ "scope": scope, "direct_children": direct_children }))
        .collect()
}

fn top_impacted_scopes<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<Value> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for id in ids {
        *counts.entry(symbol_scope(id)).or_default() += 1;
    }
    let mut pairs: Vec<(String, usize)> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.into_iter()
        .take(12)
        .map(|(scope, changes)| json!({ "scope": scope, "changes": changes }))
        .collect()
}

fn symbol_scope(symbol_id: &str) -> String {
    symbol_id
        .split("::")
        .next()
        .unwrap_or(symbol_id)
        .trim_start_matches('@')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        GraphEdgeSnapshot, GraphSnapshot, compare_graph_snapshot_values, compare_variant_snapshots,
        edge_detail_key, extract_review_context, summarize_scope_children,
        summarize_variant_comparison,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    #[test]
    fn graph_delta_reports_edge_metadata_shifts() {
        let edge_key = edge_detail_key("src/lib.rs::caller", "src/lib.rs::callee", "calls");
        let baseline_edge = GraphEdgeSnapshot {
            from: "src/lib.rs::caller".to_string(),
            to: "src/lib.rs::callee".to_string(),
            kind: "calls".to_string(),
            tier: "semantic".to_string(),
            confidence: 0.62,
            source: "indexed:call_scan".to_string(),
            evidence: vec!["indexed".to_string(), "call_scan".to_string()],
        };
        let variant_edge = GraphEdgeSnapshot {
            confidence: 0.88,
            evidence: vec![
                "indexed".to_string(),
                "call_scan".to_string(),
                "same_file".to_string(),
                "unique_best".to_string(),
            ],
            ..baseline_edge.clone()
        };
        let baseline = GraphSnapshot {
            node_ids: vec![
                "src/lib.rs::caller".to_string(),
                "src/lib.rs::callee".to_string(),
            ],
            typed_edges: vec![(
                "src/lib.rs::caller".to_string(),
                "src/lib.rs::callee".to_string(),
                "calls".to_string(),
            )],
            edge_details: BTreeMap::from([(edge_key.clone(), baseline_edge)]),
            summary: json!({"origin": "indexed"}),
        };
        let variant = GraphSnapshot {
            node_ids: baseline.node_ids.clone(),
            typed_edges: baseline.typed_edges.clone(),
            edge_details: BTreeMap::from([(edge_key, variant_edge)]),
            summary: json!({"origin": "indexed"}),
        };

        let delta = compare_graph_snapshot_values(&baseline, &variant);
        let changed = delta["changed_edges"].as_array().expect("changed_edges");
        assert_eq!(changed.len(), 1, "{delta}");
        assert_eq!(changed[0]["kind"], "calls");
        assert!(changed[0]["confidence_delta"].as_f64().unwrap_or(0.0) > 0.2);
        assert_eq!(changed[0]["evidence_changed"], true);
        assert_eq!(changed[0]["source_changed"], false);
        assert_eq!(delta["risk_summary"]["severity"], "high");
        assert!(
            delta["risk_summary"]["reasons"]
                .as_array()
                .map(|reasons| reasons.iter().any(|reason| reason == "unstable_edge_changes:1"))
                .unwrap_or(false),
            "{delta}"
        );
    }

    #[test]
    fn pairwise_variant_deltas_are_emitted() {
        let left = GraphSnapshot {
            node_ids: vec!["a::x".to_string()],
            typed_edges: vec![],
            edge_details: BTreeMap::new(),
            summary: json!({"origin": "indexed"}),
        };
        let right = GraphSnapshot {
            node_ids: vec!["a::x".to_string(), "a::y".to_string()],
            typed_edges: vec![],
            edge_details: BTreeMap::new(),
            summary: json!({"origin": "indexed"}),
        };
        let deltas = compare_variant_snapshots(&[
            (Uuid::nil(), "left".to_string(), left),
            (Uuid::from_u128(1), "right".to_string(), right),
        ]);
        assert_eq!(deltas.len(), 1, "{deltas:?}");
        assert_eq!(deltas[0]["left_title"], "left");
        assert_eq!(deltas[0]["right_title"], "right");
        assert_eq!(deltas[0]["delta"]["added_nodes"], json!(["a::y"]));
    }

    #[test]
    fn scope_children_summary_counts_direct_contains_edges() {
        let summary = summarize_scope_children(&BTreeMap::from([
            (
                edge_detail_key("src/lib.rs::outer", "src/lib.rs::inner", "contains"),
                GraphEdgeSnapshot {
                    from: "src/lib.rs::outer".to_string(),
                    to: "src/lib.rs::inner".to_string(),
                    kind: "contains".to_string(),
                    tier: "structural".to_string(),
                    confidence: 0.95,
                    source: "indexed:contains".to_string(),
                    evidence: vec![],
                },
            ),
            (
                edge_detail_key("src/lib.rs::outer", "src/lib.rs::other", "contains"),
                GraphEdgeSnapshot {
                    from: "src/lib.rs::outer".to_string(),
                    to: "src/lib.rs::other".to_string(),
                    kind: "contains".to_string(),
                    tier: "structural".to_string(),
                    confidence: 0.95,
                    source: "indexed:contains".to_string(),
                    evidence: vec![],
                },
            ),
        ]));
        assert_eq!(summary, vec![json!({"scope": "src/lib.rs", "direct_children": 2})]);
    }

    #[test]
    fn variant_comparison_summary_ranks_by_risk_and_surfaces_common_scopes() {
        let snapshots = vec![
            (Uuid::nil(), "a".to_string(), GraphSnapshot {
                node_ids: vec![],
                typed_edges: vec![],
                edge_details: BTreeMap::new(),
                summary: json!({"origin": "indexed"}),
            }),
            (Uuid::from_u128(1), "b".to_string(), GraphSnapshot {
                node_ids: vec![],
                typed_edges: vec![],
                edge_details: BTreeMap::new(),
                summary: json!({"origin": "indexed"}),
            }),
        ];
        let variants = vec![
            json!({
                "variant_id": Uuid::nil(),
                "title": "a",
                "summary": {
                    "files_changed": 1,
                    "graph_delta": {
                        "risk_summary": {"severity": "medium"},
                        "top_impacted_scopes": [{"scope": "src/lib.rs", "changes": 2}]
                    }
                }
            }),
            json!({
                "variant_id": Uuid::from_u128(1),
                "title": "b",
                "summary": {
                    "files_changed": 4,
                    "graph_delta": {
                        "risk_summary": {"severity": "high"},
                        "top_impacted_scopes": [{"scope": "src/lib.rs", "changes": 1}]
                    }
                }
            }),
        ];
        let summary = summarize_variant_comparison(&variants, &snapshots);
        assert_eq!(summary["variant_count"], 2);
        assert_eq!(summary["rankings"][0]["title"], "b");
        assert_eq!(summary["common_impacted_scopes"], json!([{"scope": "src/lib.rs", "variants": 2}]));
    }

    #[test]
    fn review_context_extracts_graph_risk_and_impacted_scopes() {
        let summary = json!({
            "files_changed": 3,
            "added": 1,
            "removed": 0,
            "modified": 2,
            "graph_delta": {
                "risk_summary": {"severity": "high", "reasons": ["unstable_edge_changes:2"]},
                "top_impacted_scopes": [{"scope": "src/lib.rs", "changes": 4}]
            }
        });
        let context = extract_review_context(&summary);
        assert_eq!(context["graph_risk"]["severity"], "high");
        assert_eq!(context["impacted_scopes"][0]["scope"], "src/lib.rs");
        assert_eq!(context["files_changed"], 3);
    }
}

fn populate_file_map(root: &Path, current: &Path, out: &mut HashMap<String, String>) -> Result<()> {
    let mut builder = WalkBuilder::new(current);
    builder.hidden(false).git_ignore(true).ignore(true).parents(false);
    builder.filter_entry(|entry| {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".git" || name == ".curd" {
                return false;
            }
        }
        true
    });
    for entry in builder.build().flatten() {
        let path = entry.path();
        if path.is_file()
            && let Ok(rel) = path.strip_prefix(root)
            && let Some(hash) = hash_file(path)
        {
            out.insert(rel.to_string_lossy().to_string(), hash);
        }
    }
    Ok(())
}
