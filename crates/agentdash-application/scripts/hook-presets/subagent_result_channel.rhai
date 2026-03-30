// subagent_result_channel — 处理子 Agent 回流结果

let p = ctx.payload;
if p == () {
    return #{};
}

let summary = p.summary;
if summary == () || summary == "" {
    return #{};
}

let stype = if ctx.subagent_type != () { ctx.subagent_type } else { "companion" };
let status = if p.status != () { p.status } else { "completed" };
let csid = if p.companion_session_id != () { p.companion_session_id } else { "-" };
let adoption = if p.adoption_mode != () { p.adoption_mode } else { "suggestion" };
let did = if p.dispatch_id != () { p.dispatch_id } else { "-" };

let findings = if p.findings != () { p.findings } else { [] };
let follow_ups = if p.follow_ups != () { p.follow_ups } else { [] };
let artifact_refs = if p.artifact_refs != () { p.artifact_refs } else { [] };

let src = ctx.workflow.source;

let base_diag = make_diagnostic(
    "subagent_result_recorded",
    "已收到 `" + stype + "` 的回流结果：" + summary
        + " (status=" + status
        + ", adoption_mode=" + adoption
        + ", companion_session_id=" + csid
        + ", dispatch_id=" + did + ")"
);

if adoption != "follow_up_required" && adoption != "blocking_review" {
    return #{
        diagnostics: [base_diag]
    };
}

let is_blocking = adoption == "blocking_review";

// 构建 workflow context 文本
let header = if is_blocking { "## Companion Blocking Review" } else { "## Companion Follow-up" };
let sections = header
    + "\n- 类型: " + stype
    + "\n- status: " + status
    + "\n- dispatch_id: " + did
    + "\n- companion_session_id: " + csid
    + "\n- 摘要: " + summary;

if !findings.is_empty() {
    sections += "\n\n### 关键发现";
    for f in findings { sections += "\n- " + f; }
}
if !follow_ups.is_empty() {
    sections += "\n\n### 建议后续动作";
    for f in follow_ups { sections += "\n- " + f; }
}
if !artifact_refs.is_empty() {
    sections += "\n\n### 相关产物";
    for a in artifact_refs { sections += "\n- " + a; }
}

sections += if is_blocking {
    "\n\n请先明确这份 companion 结果如何被主 session 采纳、拒绝或拆解，不要直接忽略后继续结束本轮。"
} else {
    "\n\n请把这份 companion 结果吸收进主 session 的下一步行动中，再继续推进。"
};

let constraint_text = if is_blocking {
    "当前 `" + stype + "` 回流被标记为 blocking_review；主 session 必须先明确采纳/拒绝/拆解该结果，再继续其它推进或自然结束。"
} else {
    "当前 `" + stype + "` 回流要求 follow-up；主 session 需要先吸收结果并落实下一步动作，再继续推进。"
};

let extra_diag_code = if is_blocking {
    "subagent_result_blocking_review_enqueued"
} else {
    "subagent_result_follow_up_enqueued"
};
let extra_diag_msg = if is_blocking {
    "已把 companion 回流升级为阻塞式 review 待办，要求主 session 优先处理"
} else {
    "已把 companion 回流升级为 follow-up 待办，要求主 session 继续处理"
};

#{
    inject: [
        make_injection("workflow", sections, src),
        make_injection("constraint", constraint_text, src)
    ],
    diagnostics: [
        base_diag,
        make_diagnostic(extra_diag_code, extra_diag_msg)
    ]
}
