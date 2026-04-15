// port_output_gate — 检查 output port 交付状态，未满足时阻止推进
//
// 在 agent 调用 advance_lifecycle_node 之前运行（BeforeStop 触发器）。
// 读取 snapshot 中的 output_port_keys（要求产出）与 fulfilled_port_keys（已产出），
// 对缺失的 port 返回 gate 拒绝。

let node_type = ctx.workflow.node_type;
if node_type != "agent_node" {
    return #{};
}

let output_ports = ctx.workflow.output_port_keys;
if output_ports == () || output_ports.len() == 0 {
    return #{};
}

let fulfilled = if ctx.workflow.fulfilled_port_keys != () {
    ctx.workflow.fulfilled_port_keys
} else {
    []
};

let missing = [];
for port_key in output_ports {
    let found = false;
    for fk in fulfilled {
        if fk == port_key {
            found = true;
            break;
        }
    }
    if !found {
        missing.push(port_key);
    }
}

if missing.len() == 0 {
    return #{};
}

let step_key = if ctx.workflow.step_key != () { ctx.workflow.step_key } else { "unknown" };
let collision_count = if ctx.workflow.gate_collision_count != () { ctx.workflow.gate_collision_count } else { 0 };
let src = ctx.workflow.source;
let missing_list = missing.reduce(|acc, k| acc + ", " + k, "");

#{
    completion: #{
        mode: "stop_gate",
        satisfied: false,
        reason: "Output port 交付未满足: [" + missing_list + "]"
    },
    inject: [
        make_injection("workflow",
            "## Port Output Gate\n当前 lifecycle node `" + step_key + "` 有 **" + missing.len() + "** 个 output port 尚未交付：\n- " + missing.reduce(|acc, k| acc + "\n- " + k, "") + "\n\n请通过 `write_file` 工具将产出写入 `lifecycle://artifacts/{port_key}` 路径，每个 port_key 对应一份产出。\n\n已碰撞门禁 " + collision_count + " 次，超过 3 次将自动标记为失败。",
            src)
    ],
    diagnostics: [
        make_diagnostic(
            "port_output_gate_unsatisfied",
            "Node `" + step_key + "` output port 未满足: [" + missing_list + "]（collision=" + collision_count + "）"
        )
    ]
}
