# Dynamic Script Artifact Compiler 实施计划

## 阶段

### Phase 1：脚本语法与 AST 评审

1. 基于 `research/claude-workflow-behavior-coverage.md` 列出必须覆盖的脚本行为。
2. 提供 2-3 个候选语法：
   - JSON/YAML DSL
   - restricted Rhai-like DSL
   - TypeScript-like declarative DSL
3. 为每个候选写同一组示例：phase、parallel agent fanout、pipeline、human gate、function/local effect、state variable。
4. 和用户评审后确定首版语法。

停止点：语法形态和 AST 合同确认前不进入代码实现。

### Phase 2：领域合同与资产边界

1. 新增 `RunScriptArtifact` / `WorkflowScriptDefinition` value object 或 entity。
2. 明确 digest、revision、approval provenance、args schema、limits 和 capability summary 字段。
3. 如需要持久化，新增 migration；字段不使用 `_json` / `_jsonb` 后缀。
4. 更新 Shared Library / Project asset 边界文档。

### Phase 3：ScriptCompiler

1. 新增 `workflow/orchestration/script_compiler.rs` 或等价 application 模块。
2. parser 产生 AST；compiler 只消费 AST 并输出 `OrchestrationPlanSnapshot`。
3. 复用 WorkflowGraph compiler 的 digest / diagnostics / canonical plan helper。
4. 编译 mappings：
   - phase -> phase node / path prefix
   - agent -> AgentCall
   - function -> Function
   - local_effect -> LocalEffect
   - human_gate -> HumanGate
   - parallel/pipeline/if -> ActivationRule
   - variable/artifact -> StateExchangeRule
5. 添加 fixtures 和 deterministic digest 测试。

### Phase 4：审批与启动 API

1. 新增 draft create / preflight / approve / launch API。
2. API 返回 source、diagnostics、plan preview、capability summary。
3. approve 后创建 `OrchestrationInstance(role=dynamic_script)` 并交给 existing runtime drain。
4. 前端先做最小 preview，不做复杂可视化编辑器。

### Phase 5：保存为 workflow

1. 从 `RunScriptArtifact` 创建 `WorkflowScriptDefinition` revision。
2. 保存动作不修改已运行 `OrchestrationInstance.plan_snapshot`。
3. 资产安装/更新路径与 Shared Library contract 对齐。

## 首批 fixtures

- 单 agent：一个 agent node 输出 `result`。
- pipeline：agent -> function API -> human gate。
- parallel：三个 agent 分支 fanout，barrier join 后汇总。
- local effect：BashExec 写入 output port。
- args/state：脚本参数进入 agent prompt，agent output 绑定到后继 input。
- diagnostics：未知变量、重复 node name、无界 fanout、缺失 capability declaration。

## 验证命令

按实现范围缩小，预期至少：

```powershell
cargo test -p agentdash-domain script
cargo test -p agentdash-application script_compiler
cargo test -p agentdash-application orchestration
pnpm run contracts:check
pnpm run frontend:check
pnpm run migration:guard
git diff --check
```

涉及真实 launch 时再运行：

```powershell
pnpm dev
```

## 需要读取的上下文

1. 父任务 `prd.md`、`design.md`、`target-model-sketch.md`。
2. `research/claude-workflow-behavior-coverage.md`。
3. `research/common-runtime-convergence-plan.md`。
4. `research/workflow-graph-compiler-plan.md`。
5. `../06-06-common-orchestration-runtime-static-graph/` 的最终 PRD / design / implement。
6. `.trellis/spec/backend/workflow/activity-lifecycle.md`。
7. `.trellis/spec/cross-layer/frontend-backend-contracts.md`。

## 当前建议

下一步不要直接写 parser。先产出三份候选脚本语法示例，并用同一组 Claude Workflow 行为矩阵评估可读性、可审性、compiler 难度和运行时可解释性。
