// subagent_record_result — 子 Agent 派发完成后记录诊断信息

let stype = ctx.subagent_type;
if stype == () || stype == "" {
    return #{};
}

let p = ctx.payload;
let csid = if p != () && p.companion_session_id != () { p.companion_session_id } else { "-" };
let tid = if p != () && p.turn_id != () { p.turn_id } else { "-" };

#{
    diagnostics: [
        make_diagnostic(
            "after_subagent_dispatch_recorded",
            "已记录 `" + stype + "` 的 subagent dispatch 结果 (companion_session_id=" + csid + ", turn_id=" + tid + ")"
        )
    ]
}
