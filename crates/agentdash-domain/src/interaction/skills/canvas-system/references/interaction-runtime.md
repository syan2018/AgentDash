# Canvas Interaction Runtime

## Runtime 模型

展示 Canvas definition 时，Canvas provider 创建或复用 `InteractionInstance`，并 attach 当前 actor。
instance 拥有 canonical shared state 与单调递增的 state revision。renderer lease、Workspace tab
与 attachment 具有不同生命周期。

仅在当前 Workspace Module surface 返回 `interaction:{instance_id}` 时使用它。把 descriptor 中的
Agent state projection 作为只读上下文。

## Command

修改状态前：

1. describe 当前 `interaction:{instance_id}` module。
2. 选择 `agent_and_panel` 且 `ready` 的 command Operation。
3. 复制完整 OperationRef，并遵守 input schema。
4. 携带 descriptor 要求的准确 expected state revision。
5. 成功或发生 revision conflict 后重新 describe。

command 拥有确定性 state transition。不要把 projected state、component props 或 presentation
payload 当作 write authority。

## Component 与 event

Canvas component binding 引用显式声明的 Extension component 与固定 component ABI。component
event 通过声明的 event binding 进入 trusted host。binding 可以指向：

- versioned platform command；
- 一个准确 Operation；
- 有界的 ephemeral OperationScript。

iframe 不接收 credential、backend ID、placement fact、authorization revision 或 host filesystem
path。host 解析 authority，并重新准入每次 Operation。

OperationScript 只用于有界即时组合。durable retry、recovery、human gate 或跨 session 编排使用
Workflow。

Canvas source 中的按钮通过 `window.agentdash.actions.invoke(actionKey, payload)` 触发
当前 immutable definition 的 action binding。普通 Canvas action 不依赖 Extension UI component
artifact；`component_bindings` 只负责已安装 Extension component 的 artifact pinning 和事件契约。
需要读取 attached AgentRun 的 `main` VFS 时，host
先验证当前用户、active attachment 与 AgentRun owner，再以该 Agent 的 current Operation surface
执行 binding 中固定的 OperationScript；iframe 不能提交任意脚本或扩大 `requested_operations`。

### 实时读取 attached Agent 的 skills

definition 在 `action_bindings` 中固定 `skills.refresh`，target 使用 SourceBundle 内的
`actions/load-skills.rhai`，并只授予：

- `platform:vfs:fs_glob:v1`
- `platform:vfs:fs_read:v1`

脚本先读取 `main://**/SKILL.md`，再把所有文件读取收束进一次 `ops.invoke_all`：

```rhai
fn frontmatter_field(text, key) {
    let prefix = key + ":";
    for line in text.split("\n") {
        if line.starts_with(prefix) {
            let conventional = prefix + " ";
            if line.starts_with(conventional) {
                let value = line;
                value.replace(conventional, "");
                return value;
            }
            let value = line;
            value.replace(prefix, "");
            return value;
        }
    }
    ""
}

let found = ops.invoke(
    "platform:vfs:fs_glob:v1",
    #{ path: "main://", pattern: "**/SKILL.md" }
);
let requests = [];
for path in found.content[0].text.split("\n") {
    if path.ends_with("SKILL.md") {
        requests.push(#{
            operation: "platform:vfs:fs_read:v1",
            input: #{ path: "main://" + path }
        });
    }
}
let files = ops.invoke_all(requests);
let skills = [];
for index in 0..files.len() {
    let text = files[index].content[0].text;
    skills.push(#{
        path: requests[index].input.path,
        name: frontmatter_field(text, "name"),
        description: frontmatter_field(text, "description")
    });
}
#{ skills: skills }
```

Canvas 按钮只调用 `window.agentdash.actions.invoke("skills.refresh", {})`，直接渲染脚本返回的
`skills[{path,name,description}]`。VFS 路径、Agent authority 和 exact Operation manifest 都由
host/definition 控制，源码不携带 AgentRun identity；脚本只返回展示所需字段，避免把全部 Skill
正文穿过 runtime bridge。

## Resource slot

通过声明的 resource slot 与当前 attachment authority 绑定数据。区分 definition-level、
shared-instance 与 attachment-local binding。binding 不是 Canvas source，也不会隐式修改 canonical
Interaction state。
