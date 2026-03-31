// stop_gate_checks_pending — 在 completion checks 全部满足前阻止 Agent 结束 session
//
// 此 preset 必须由 workflow 定义方在 hook_rules 中显式声明。
// 效果：产出 inject（推动 Agent 继续补齐条件）+ completion gate（通知框架阻止结束）。

if !ctx.signals.auto_completion {
    return #{};
}
if !ctx.signals.has_block_stop_constraint {
    return #{};
}
if ctx.signals.completion_satisfied {
    return #{};
}

let src = ctx.workflow.source;
let step_key = if ctx.workflow.step_key != () { ctx.workflow.step_key } else { "unknown" };
let evidence = ctx.signals.checklist_evidence_present;

#{
    completion: #{
        mode: "stop_gate",
        satisfied: false,
        reason: "Waiting for completion checks to pass"
    },
    inject: [
        make_injection("workflow",
            "## Session Stop Gate\n当前 workflow step `" + step_key + "` 的 completion checks 尚未满足，不允许结束 session。\n\n**请立即执行以下动作之一**：\n1. 如果有需要补齐的验证结论或证据，请继续完成并使用 `report_workflow_artifact` 提交产物\n2. 如果当前任务已无法推进，请明确说明阻塞原因\n3. 不要尝试结束 session，也不要重复总结前文",
            src),
        make_injection("constraint",
            "当前 step 的 contract checks 还未满足（evidence_present=" + evidence + "）。请先补齐验证结论、必要证据与状态信号，再结束 session。",
            src)
    ],
    diagnostics: [
        make_diagnostic(
            "before_stop_workflow_checks_pending",
            "Stop gate active: workflow step `" + step_key + "` checks not satisfied (evidence=" + evidence + "), injecting resume context"
        )
    ]
}
