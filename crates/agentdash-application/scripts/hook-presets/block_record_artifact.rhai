// block_record_artifact — 在当前 step 禁止上报特定类型的 workflow artifact
//
// params.artifact_types: 被禁止的 artifact type 列表

let blocked = ctx.params.artifact_types;
if blocked == () || blocked.is_empty() {
    return #{};
}

let tool = ctx.tool_name;
if tool == () || !is_workflow_artifact_tool(tool) {
    return #{};
}

let args = ctx.payload.args;
if args == () {
    return #{};
}
let artifact_type = args.artifact_type;
if artifact_type == () {
    return #{};
}

for bt in blocked {
    if bt == artifact_type {
        let src = ctx.workflow.source;
        return #{
            block: "当前 workflow contract 禁止在此 step 上报该类记录产物，请先满足当前 step 的收口要求。",
            diagnostics: [
                make_diagnostic(
                    "before_tool_record_artifact_blocked",
                    "Hook 根据 workflow contract 阻止了当前记录产物上报 (source=" + src + ")"
                )
            ]
        };
    }
}

#{}
