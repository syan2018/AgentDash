# Dynamic Script Artifact Compiler 实施计划

## 阶段

### Phase 1：公共 Rhai 内核抽取

1. 从 `RhaiHookScriptEvaluator` 中抽出 `RhaiScriptRuntime` 内核：
   - sandbox limits
   - compile / validate
   - AST cache
   - serde_json ctx/value bridge
   - helper/module registration hook
2. `RhaiHookScriptEvaluator` 改为公共内核的 adapter，并保持现有 `HookScriptEvaluator` SPI 不变。
3. 新增 workflow script evaluator SPI 草案，例如：

   ```rust
   pub trait WorkflowScriptEvaluator: Send + Sync {
       fn validate_workflow_script(&self, source: &str) -> Result<(), Vec<String>>;
       fn eval_workflow_script(
           &self,
           source: &str,
           ctx: &serde_json::Value,
       ) -> Result<serde_json::Value, String>;
   }
   ```

4. 新增 `RhaiWorkflowScriptEvaluator`，只注册 workflow builder helpers，不注册 hook decision helpers。

停止点：公共内核抽取必须先保持 Hook 现有测试通过，再进入 workflow compiler。

### Phase 2：Rhai builder DSL 与 AST 评审

1. 基于 `research/claude-workflow-behavior-coverage.md` 列出必须覆盖的脚本行为。
2. 首版语法采用 restricted Rhai builder DSL；提供同一组示例：phase、parallel agent fanout、pipeline、human gate、function/local effect、state variable。
3. 明确 helper 返回的是 serializable builder document，而不是直接执行 side effect。
4. 和用户评审 builder document shape。

停止点：语法形态和 AST 合同确认前不进入代码实现。

### Phase 3：领域合同与资产边界

1. 新增 `RunScriptArtifact` / `WorkflowScriptDefinition` value object 或 entity。
2. 明确 digest、revision、approval provenance、args schema、limits 和 capability summary 字段。
3. `RunScriptArtifact` 首版跟随 Lifecycle 保存；除非审批列表和跨 Lifecycle 查询已经需要，否则不拆独立 repository。
4. 全局注册为 `WorkflowScriptDefinition` 作为后续能力；首批只保留边界和 contract，不要求实现 projection。
5. 如需要持久化，新增 migration；字段不使用 `_json` / `_jsonb` 后缀。

### Phase 4：ScriptCompiler

1. 新增 `workflow/orchestration/script_compiler.rs` 或等价 application 模块。
2. `RhaiWorkflowScriptEvaluator` 产生 builder document；compiler 只消费 builder document / typed AST 并输出 `OrchestrationPlanSnapshot`。
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

### Phase 5：审批与启动 API

1. 新增 draft create / preflight / approve / launch API。
2. API 返回 source、diagnostics、plan preview、capability summary。
3. approve 后创建 `OrchestrationInstance(role=dynamic_script)` 并交给 existing runtime drain。
4. 前端先做最小 preview，不做复杂可视化编辑器。

### Phase 6：保存为 workflow

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
cargo test -p agentdash-infrastructure rhai
cargo test -p agentdash-application hooks
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
8. `.trellis/spec/backend/hooks/hook-script-engine.md`。

## 当前建议

下一步不要直接写 parser。先做公共 Rhai 内核抽取设计和 Rhai builder DSL 示例，确认 Hook adapter 与 Workflow adapter 的 surface 隔离，再进入 ScriptCompiler。
