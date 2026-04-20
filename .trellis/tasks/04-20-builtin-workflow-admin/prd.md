# 内建工作流：工作流管理 Agent 能力绑定

## Goal

新增一个**专门用于"管理项目工作流"的内建工作流（builtin workflow + lifecycle）**，放在 `crates/agentdash-application/src/workflow/builtins/` 下。Agent session 绑定该工作流后，自动获得 `workflow_management` 能力，可直接调用 `WorkflowMcpServer` 暴露的 Workflow / Lifecycle CRUD 工具集，以自然语言协同完成"创建、调整、校验项目工作流定义"。

该任务同时重构 `CapabilityVisibilityRule` 的判定语义：**允许信号（agent 声明 / workflow 授予）走 OR**，**屏蔽信号（owner 类型等硬边界）走 AND**。这样既能让"绑定内建工作流即赋能"成立，又保留既有 agent 声明路径（向后兼容）。

## Requirements

### 新增内建工作流 JSON

在 [crates/agentdash-application/src/workflow/builtins/](crates/agentdash-application/src/workflow/builtins/) 下新增 `builtin_workflow_admin.json`（或相近命名），包含：

- **binding_kind**: `project`（与 WorkflowMcpServer 的 Project 粒度一致）
- **2 个原子 workflow**：
  - `builtin_workflow_admin_plan` —— 理解现状与方案设计；通过 `injection.instructions` 引导先 Get/List 现有工作流、识别缺口、给出设计草稿；不通过 constraint 做运行时硬拦截（见 Decision）
  - `builtin_workflow_admin_apply` —— 执行 Upsert 与校验；`injection.instructions` 强调必须先 GetWorkflow 参考现状再 Upsert、失败后根据 ValidationResponse 修正
- **Lifecycle**：`entry_step_key = plan`，两步 `plan → apply`（边用 Port 级 edge 显式连接），步骤级 `capabilities` 使用 `CapabilityDirective`：
  - `plan.capabilities`: `Add(workflow_management)`
  - `apply.capabilities`: `Add(workflow_management)` 显式续约（两步能力相同，差异仅在 instructions）
- **Context bindings**（注入给 agent 的参考文档）：
  - `.trellis/spec/backend/tool-capability-pipeline.md`（能力协议）
  - 至少 1 个现有 builtin JSON 作为格式范例（如 `trellis_dev_task.json`）
  - 可能需要补一份 `workflow-authoring-guide.md` spec（见 Open Follow-ups）

### 重构 Visibility Rule 语义（允许 OR / 屏蔽 AND）

**核心语义升级**：把 visibility 判定从"所有字段 AND"改为分两类：

- **允许信号（positive / "授予"）**：`requires_agent_declaration` 与 `requires_workflow`
  - 语义转换：从"要求"改为"授予源"—— agent 声明 OR workflow 授予，**任一**为真即视为被授予
  - 规则字段建议改名：`agent_can_grant: bool`、`workflow_can_grant: bool`（默认 true）
- **屏蔽信号（negative / "禁止"）**：`allowed_owner_types` 等硬边界
  - 仍走 AND：所有硬边界都必须满足才能可见
  - workflow_management 保持 `allowed_owner_types: [Project]`

**`is_capability_visible` 新逻辑（伪代码）**：

```
if cap.is_custom_mcp(): return true
rule = find_rule(cap) or return false

# 屏蔽 AND：任一硬边界不满足即拒绝
if owner_type not in rule.allowed_owner_types: return false

# 授予 OR：至少一个 grant 源命中
granted = (rule.agent_can_grant && agent_declares)
       || (rule.workflow_can_grant && has_active_workflow)
return granted
```

**workflow_management 规则新值**：
- `allowed_owner_types = [Project]`（硬屏蔽，不变）
- `agent_can_grant = true`（保留向后兼容，既有 agent 声明路径不失效）
- `workflow_can_grant = true`（新增路径，让绑定内建工作流即赋能）

**配套**：
- 既有 well-known capability 默认规则迁移：把原 `requires_agent_declaration/requires_workflow: true` 映射为新字段（详见 Technical Notes 的迁移表）
- 单测：
  - 所有既有 well-known capability 的可见性结果在迁移后**不回归**（回放测试）
  - workflow_management 的新路径专项测：{agent_declares=false, has_active_workflow=true} → 可见
  - 屏蔽路径专项测：Task owner + 所有 grant 全给 → 仍不可见
- 扫调用方：`CapabilityResolver` 以及 `agentdash-application/src/capability/*` 里直接读 `requires_*` 字段的代码都要改到新字段

### 扩展 WorkflowConstraintSpec 支持写操作禁用

（已移除）— plan 步的只读语义**不通过 constraint 层的工具黑名单实现**。constraints 是全局机制，不应为特定 MCP 工具名做定制化 policy；这种分权诉求的正确落点是 capability 粒度（拆 `workflow_management_read`/`_write`），不是 constraint 层。因此 MVP 改为：

- plan 步和 apply 步**都拿完整 `workflow_management`**
- plan / apply 的差异化完全通过 `injection.instructions` 表达（"plan 阶段先 Get/List 分析，不要 Upsert；apply 阶段再执行写入"）
- 运行时不做硬拦截；若未来确需硬分权，另立任务做 capability 拆分

### BuiltinWorkflowBundle 加载覆盖

- JSON 放到 builtins 目录即自动通过 `include_str!` / registry 机制加载（先确认现有机制）
- 若需手动登记到 bundle 清单（数组），补充登记代码
- BuiltinWorkflowBundle 的加载测试覆盖新 JSON 的解析与注册

## Acceptance Criteria

- [ ] 后端启动后，数据库 `workflow_definition` / `lifecycle_definition` 表里能查到新的 builtin key（`builtin_workflow_admin_plan`、`builtin_workflow_admin_apply`、`builtin_workflow_admin` lifecycle）
- [ ] Project session 绑定该 lifecycle 后，`CapabilityResolver` 解析出的 `FlowCapabilities` 包含 `workflow_management`（即使 agent config 未声明）
- [ ] 既有路径不回归：agent config 声明 workflow_management 但未绑定该 lifecycle 的 Project session，workflow_management 仍可见
- [ ] 屏蔽路径成立：Task 或 Story owner 即使满足所有 grant 源，workflow_management 仍不可见
- [ ] MCP scope 解析正确，`WorkflowMcpServer` 的 5 个工具（List、GetWorkflow、GetLifecycle、UpsertWorkflow、UpsertLifecycle）在 agent 会话内可见
- [ ] Agent 在 plan / apply 两步都能看到 `WorkflowMcpServer` 的 5 个工具（不做运行时硬拦截）；plan 步的只读行为由 `injection.instructions` 引导，人工 demo 验证 agent 遵循节奏
- [ ] `default_visibility_rules` 迁移到新字段；`is_capability_visible` 的回放测试覆盖所有既有 well-known capability 无回归
- [ ] 既有单测 / 集成测试（capability/pipeline_tests）全绿
- [ ] `cargo build`、`cargo test`、`cargo clippy` 全绿；前端 `npm run build` 通过
- [ ] 前端 Project 工作流面板能看到新 lifecycle 名称，且绑定后会话 UI 的能力面板反映出 `workflow_management`

## Definition of Done

- 代码与 JSON 变更通过 lint、typecheck、cargo test、frontend build
- BuiltinWorkflowBundle 单测覆盖新 JSON 加载
- visibility rule 变更有专门单测
- 手动 demo 一次：Project session → 绑定此 lifecycle → Agent 在 plan 拿到只读工具、apply 拿到写工具 → 成功新建一个测试工作流
- PRD 更新完毕

## Technical Approach

**JSON 骨架（示意）**：

```json
{
  "key": "builtin_workflow_admin",
  "name": "Builtin / Workflow Admin",
  "description": "以自然语言协同管理 Project 级工作流定义（plan → apply）。",
  "binding_kind": "project",
  "recommended_binding_roles": ["project"],
  "workflows": [
    {
      "key": "builtin_workflow_admin_plan",
      "contract": {
        "injection": {
          "instructions": [
            "当前处于 Plan 阶段。先用 GetWorkflow/GetLifecycle/List 掌握现状，分析缺口。",
            "本阶段只做理解与设计，暂不执行 UpsertWorkflow/UpsertLifecycle；进入 Apply 阶段后再写入。"
          ],
          "context_bindings": [
            {"locator": ".trellis/spec/backend/tool-capability-pipeline.md", "required": true},
            {"locator": "crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json", "required": true, "title": "Builtin JSON 范例"}
          ]
        }
      }
    },
    {"key": "builtin_workflow_admin_apply", "contract": {"injection": {"instructions": ["..."]}}}
  ],
  "lifecycle": {
    "key": "builtin_workflow_admin",
    "entry_step_key": "plan",
    "steps": [
      {"key": "plan", "workflow_key": "builtin_workflow_admin_plan",
       "capabilities": [{"add": "workflow_management"}]},
      {"key": "apply", "workflow_key": "builtin_workflow_admin_apply",
       "capabilities": [{"add": "workflow_management"}]}
    ],
    "edges": [{"from": {"step": "plan", "port": "next"}, "to": {"step": "apply", "port": "in"}}]
  }
}
```

（具体字段名以现有 schema 为准——实现时先对齐 `trellis_dag_task.json` / WorkflowMcpServer UpsertLifecycleParams 的真实结构）

**plan 只读的实现**：仅通过 `injection.instructions` 引导 agent 在 plan 阶段只读、apply 阶段再写。**不在 constraints 层做工具黑名单**（避免"为单个 MCP 工具名定制 policy"的过度设计）。若未来确需硬分权，正确路径是拆 `workflow_management_read/_write` 两个 well-known capability，这属于全局性能力模型调整，另立任务。

**Visibility rule 变更**：`requires_workflow: true, requires_agent_declaration: false`；单测验证"绑定此 workflow 的 Project session → workflow_management 可见"。

## Decision (ADR-lite)

**Context**: 前端目前没有给 Agent 配置 `workflow_management` 能力的 UI 入口，现状下"Agent 能操作工作流"唯一路径是改 agent config JSON；用户希望通过"工作流绑定"自然地赋予/收回此能力。但粗暴地把 `requires_agent_declaration` 改成 false 会丢掉既有声明路径的向后兼容性。

**Decision**:
1. **重构 visibility 语义**：把 `CapabilityVisibilityRule` 字段按"授予 / 屏蔽"分两类——授予走 OR（agent 声明 OR workflow 授予任一命中即授予），屏蔽走 AND（所有硬边界都要满足）
2. 新增一个两步 lifecycle 的内建工作流（plan → apply）
3. 在 workflow_management 规则里 **同时开启** `agent_can_grant` 和 `workflow_can_grant`，既走通新路径又保留旧路径
4. plan / apply 的节奏**只靠 `injection.instructions` 表达**，不在 constraint 层做工具名黑名单；硬分权需求若成立，由 capability 拆分承接

**Consequences**:
- + 用户获得"绑工作流即赋能"的直观体验；同时既有 agent config 声明路径不失效
- + Visibility 语义清晰化，后续新增 well-known capability 时可按"授予/屏蔽"明确建模
- + 能力收回自然发生于工作流切换 / 解绑
- + constraints 机制保持全局通用，不被特例工具污染
- − `CapabilityVisibilityRule` 字段改名带来全局调用方迁移（受影响面相对小，基本在 capability/ 模块内）
- − plan 步只读属于"君子协定"，运行时不拦截；demo 需观察 agent 是否遵守节奏，否则走 capability 拆分的后续任务
- − 迁移既有默认规则时需要回归测试，确保其他 capability 的可见性判定结果不变

## Out of Scope (explicit)

- **Agent 直接绑单 workflow，运行时自动展开单步 lifecycle**：现有运行时要求显式 lifecycle，这个 fallback 能力是独立的基础设施扩展，另立任务。
- **`workflow_management` 拆分为 read/write 两个细粒度 capability**：MVP 里 plan/apply 共享同一 capability，节奏只靠 instructions。若未来出现硬分权需求再拆，属于全局能力模型调整，另立任务。
- **前端给 Agent 配置能力的 UI 入口**：visibility rule 调整后不再需要，但若产品后续决定恢复 agent config 路径，再独立设计。
- **多 Agent 协作管理工作流**（例如 plan 一个 agent、apply 另一个 agent）：超出单工作流建模范畴。
- **非 Project 级 owner 获得 workflow_management**：保持 `allowed_owner_types: [Project]`。

## Technical Notes

- Builtin 目录：[crates/agentdash-application/src/workflow/builtins/](crates/agentdash-application/src/workflow/builtins/)
- MCP Server：[crates/agentdash-mcp/src/servers/workflow.rs](crates/agentdash-mcp/src/servers/workflow.rs)（5 个工具）
- Capability 定义 / visibility：[crates/agentdash-spi/src/tool_capability.rs](crates/agentdash-spi/src/tool_capability.rs)
- Capability Resolver：[crates/agentdash-application/src/capability/resolver.rs](crates/agentdash-application/src/capability/resolver.rs)
- Capability 现有测试：[crates/agentdash-application/src/capability/pipeline_tests.rs](crates/agentdash-application/src/capability/pipeline_tests.rs)
- 关联任务：
  - `04-15-workflow-dynamic-lifecycle-context`（lifecycle 上下文模型）
  - `04-20-dynamic-capability-followup`（能力链路收尾）
- 相关 Spec：`.trellis/spec/backend/tool-capability-pipeline.md`

1. **Builtin 加载需要手动登记**：`list_builtin_workflow_templates()` 在 [workflow/definition.rs:93-99](crates/agentdash-application/src/workflow/definition.rs#L93-L99) 用 `include_str!` 硬编码数组，新增 JSON 必须加到数组里。
2. **Visibility 调用面极小**：所有访问都经过 `is_capability_visible()`，唯一权威调用方是 `CapabilityResolver::default_visible_capabilities()`（[resolver.rs:180](crates/agentdash-application/src/capability/resolver.rs#L180)）。字段改名和逻辑调整的爆破面小。
3. **agent_declares 数据源**：agent config 的 `tool_clusters` 字段，传递路径不变。
4. `CapabilityDirective` 用 `#[serde(rename_all = "snake_case")]`，JSON 格式为 `{"add": "..."}` / `{"remove": "..."}`（全小写）。
5. **现有 4 个 builtin JSON 均未使用 `step.capabilities`**（全空数组 / 不声明），新任务会是**第一个**使用该字段的 builtin——值得在 PR2 做一次端到端加载测试。
