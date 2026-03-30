// supervised_tool_gate — 在 SUPERVISED 权限策略下，需要审批的工具调用进入审批流程
//
// params.allowlist: 可选，白名单工具列表（不在白名单中的执行/编辑类工具需要审批）

let policy = ctx.meta.permission_policy;
if policy == () || policy != "SUPERVISED" {
    return #{};
}

let tool = ctx.tool_name;
if tool == () {
    return #{};
}

// 检查白名单
let allowlist = ctx.params.allowlist;
if allowlist != () && !allowlist.is_empty() {
    for allowed in allowlist {
        if tool == allowed {
            return #{};
        }
    }
}

if !requires_supervised_approval(tool) {
    return #{};
}

#{
    approval: #{
        reason: "当前会话使用 SUPERVISED 权限策略，执行 `" + tool + "` 前需要用户审批。",
        details: #{
            policy: "supervised_tool_approval",
            permission_policy: policy,
            tool_name: tool
        }
    },
    diagnostics: [
        make_diagnostic(
            "before_tool_requires_approval",
            "Hook 要求在执行 `" + tool + "` 前进入人工审批"
        )
    ]
}
