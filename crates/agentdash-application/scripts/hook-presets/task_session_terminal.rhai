// task_session_terminal — Task 伴生 session 在终态时的状态转换规则
//
// 触发条件：SessionTerminal + session 绑定了 task owner
// ctx.payload: { terminal_state: "completed" | "failed" | "interrupted" }
// ctx.meta:    { task_status, task_id }
//
// 产出 effects 由 TaskHookEffectExecutor 消费执行

let terminal_state = ctx.payload.terminal_state;
let task_id = ctx.meta.task_id;

if task_id == () || task_id == "" {
    return #{};
}

let effects = [];

if terminal_state == "completed" {
    effects.push(#{ kind: "task:set_status", payload: #{ status: "awaiting_verification", reason: "turn_completed" } });
} else if terminal_state == "failed" || terminal_state == "interrupted" {
    let failure_reason = if terminal_state == "failed" { "turn_failed" } else { "turn_interrupted" };
    effects.push(#{ kind: "task:set_status", payload: #{ status: "failed", reason: failure_reason } });
}

if effects.is_empty() {
    return #{};
}

#{
    effects: effects,
    diagnostics: [
        make_diagnostic(
            "task_terminal_effects",
            `Task ${task_id} → ${terminal_state}: ${effects.len()} effect(s) emitted`
        )
    ]
}
