// subagent_inherit_context — 派发子 Agent 时继承当前 session 的注入和约束

let stype = ctx.subagent_type;
if stype == () || stype == "" {
    return #{};
}

let inherited = ctx.snapshot.injections;
if inherited == () {
    inherited = [];
}

#{
    inject: inherited,
    diagnostics: [
        make_diagnostic(
            "before_subagent_dispatch_prepared",
            "已为 `" + stype + "` 准备 companion/subagent dispatch 上下文与约束继承"
        )
    ]
}
