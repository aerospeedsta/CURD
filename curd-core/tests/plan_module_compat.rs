use curd_core::plan::{
    DslNode, IdOrTag, Plan, PlanEngine, PlanNode, ReplState, SystemEvent, SystemEventEnvelope,
    ToolOperation, now_secs,
};
use uuid::Uuid;

#[test]
fn plan_module_reexports_base_types() {
    let plan = Plan {
        id: Uuid::new_v4(),
        nodes: vec![PlanNode {
            id: Uuid::new_v4(),
            op: ToolOperation::Internal {
                command: "emit_marker".to_string(),
                params: serde_json::json!({}),
            },
            dependencies: vec![IdOrTag::Tag("root".to_string())],
            output_limit: 128,
            retry_limit: 0,
        }],
    };
    let _engine = PlanEngine {
        workspace_root: std::env::temp_dir(),
    };
    let _dsl = DslNode::Abort {
        reason: "stop".to_string(),
    };
    let _state = ReplState::new();
    let envelope = SystemEventEnvelope {
        event_id: 1,
        collaboration_id: Uuid::nil(),
        ts_secs: now_secs(),
        event: SystemEvent::PlanRegistered {
            plan_id: plan.id,
            nodes: vec![(plan.nodes[0].id, "emit_marker".to_string())],
        },
    };
    assert_eq!(envelope.event_id, 1);
    assert_eq!(plan.nodes.len(), 1);
}
