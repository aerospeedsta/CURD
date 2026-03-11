use super::types::{CanonicalOperationKind, CapabilityAtom};

pub fn canonical_op_for_tool(tool: &str) -> CanonicalOperationKind {
    match tool {
        "search" | "find" | "contract" => CanonicalOperationKind::Lookup,
        "graph" | "crawl" | "frontier" | "diagram" => CanonicalOperationKind::Traverse,
        "read" | "doc" | "lsp" | "doctor" | "profile" | "debug" => CanonicalOperationKind::Read,
        "edit" | "manage_file" | "mutate" | "proposal" | "refactor" => {
            CanonicalOperationKind::Change
        }
        "workspace" | "session" | "review_cycle" | "verify_impact" => {
            CanonicalOperationKind::Session
        }
        "shell" | "shell_status" | "terminate" | "build" => CanonicalOperationKind::Exec,
        "plugin_tool" | "plugin_language" | "plugin_trust" => CanonicalOperationKind::Other,
        "register_plan"
        | "propose_plan"
        | "execute_plan"
        | "execute_active_plan"
        | "create_plan_set"
        | "list_plan_sets"
        | "create_plan_variant"
        | "list_plan_variants"
        | "simulate_plan_variant"
        | "compare_plan_variants"
        | "review_plan_variant"
        | "promote_plan_variant"
        | "delegate"
        | "checkpoint" => CanonicalOperationKind::Plan,
        "context" => CanonicalOperationKind::Context,
        "benchmark" | "stamina" | "connection_open" | "connection_verify" | "session_open"
        | "session_verify" => CanonicalOperationKind::Other,
        _ => CanonicalOperationKind::Other,
    }
}

pub fn capability_for_tool(tool: &str) -> CapabilityAtom {
    let atom = match tool {
        "search" | "find" | "contract" => "lookup",
        "graph" | "crawl" | "frontier" | "diagram" => "traverse",
        "read" | "doc" | "lsp" | "doctor" | "profile" | "debug" => "read",
        "edit" => "change.apply",
        "manage_file" | "mutate" | "proposal" | "refactor" => "change.apply",
        "workspace" => "session.begin",
        "verify_impact" | "session" | "review_cycle" => "session.verify",
        "shell" | "shell_status" | "terminate" | "build" => "exec.task",
        "plugin_tool" | "plugin_language" => "plugin.manage",
        "plugin_trust" => "plugin.trust",
        "register_plan" | "propose_plan" => "plan.create",
        "execute_plan" | "execute_active_plan" => "plan.execute",
        "create_plan_set"
        | "create_plan_variant"
        | "list_plan_variants"
        | "list_plan_sets"
        | "simulate_plan_variant"
        | "compare_plan_variants"
        | "review_plan_variant"
        | "promote_plan_variant"
        | "delegate"
        | "checkpoint" => "plan.parallel",
        "context" => "context",
        _ => canonical_op_for_tool(tool).capability_prefix(),
    };
    CapabilityAtom::new(atom)
}
