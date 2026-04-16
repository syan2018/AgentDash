// stop_gate_lifecycle_advance — 在 agent 主动调用 complete_lifecycle_node 之前阻止结束 session
//
// 适用于 agent_node 类型的 lifecycle node（决策 D2 / D8）。
// 如果当前 node 尚未被 advance（step_status != "completed"），则 block stop。

let node_type = ctx.workflow.node_type;
if node_type != "agent_node" {
    return #{};
}

let step_status = ctx.workflow.step_status;
if step_status == "completed" || step_status == "failed" {
    return #{};
}

let step_key = if ctx.workflow.step_key != () { ctx.workflow.step_key } else { "unknown" };
let src = ctx.workflow.source;

#{
    completion: #{
        mode: "stop_gate",
        satisfied: false,
        reason: "Agent 尚未调用 complete_lifecycle_node 完成当前 node"
    },
    inject: [
        make_injection("workflow",
            "## Lifecycle Node Gate\n当前 lifecycle node `" + step_key + "` 尚未完成推进。\n\n**必须调用 `complete_lifecycle_node` 工具声明当前阶段完成后，session 才允许结束。**\n\n请确保已完成当前阶段的核心工作，然后调用该工具并附上工作摘要。",
            src)
    ],
    diagnostics: [
        make_diagnostic(
            "before_stop_lifecycle_advance_pending",
            "Lifecycle node `" + step_key + "` 未完成推进（status=" + step_status + "），阻止 session 结束"
        )
    ]
}
