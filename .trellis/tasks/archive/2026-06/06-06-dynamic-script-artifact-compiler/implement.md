# Dynamic Script Artifact Compiler 实施计划

## 阶段

### Phase 1：公共 Rhai 内核抽取（已完成）

1. 从 `RhaiHookScriptEvaluator` 中抽出 `RhaiScriptRuntime` 内核：
   - sandbox limits
   - compile / validate
   - AST cache
   - serde_json ctx/value bridge
   - helper/module registration hook
2. `RhaiHookScriptEvaluator` 改为公共内核的 adapter，并保持现有 `HookScriptEvaluator` SPI 不变。

完成状态：

- `agentdash-infrastructure::script_runtime::RhaiScriptRuntime` 已承载 Rhai engine、沙箱限制、AST cache、compile/validate/eval 和 JSON bridge。
- 模块入口为 `script_runtime/mod.rs`，Rhai 具体实现位于 `script_runtime/rhai.rs`，后续新增脚本后端或 builder adapter 时继续按目录扩展。
- `RhaiHookScriptEvaluator` 已收敛为 Hook adapter：只注册 Hook helper、维护 preset cache，并继续实现原 `HookScriptEvaluator` SPI。
- 公共内核已有基础单测覆盖 JSON ctx bridge、helper 注册和 operation limit。

已落地的 workflow script evaluator SPI：

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

停止点：公共内核抽取必须先保持 Hook 现有测试通过，再进入 workflow compiler。

### Phase 2：Rhai builder DSL 与 typed builder document（最小后端切片已完成）

1. 基于 `research/claude-workflow-behavior-coverage.md` 列出必须覆盖的脚本行为。
2. 首版语法采用 restricted Rhai builder DSL；提供同一组示例：phase、parallel agent fanout、pipeline、human gate、function/local effect、state variable。
3. 明确 helper 返回的是 serializable builder document，而不是直接执行 side effect。
4. 和用户评审 builder document shape。

完成状态：

- 新增 `WorkflowScriptEvaluator` SPI port，application 后续只依赖 port 与 builder document 合同。
- 新增 `RhaiWorkflowScriptEvaluator` infrastructure adapter，复用 `RhaiScriptRuntime`，并只注册 workflow builder helper surface。
- 首批 helper 已覆盖 `workflow`、`phase`、`log`、`agent`、`parallel`、`pipeline`、`function`、`local_effect`、`human_gate`、`api_request`、`bash_exec`、`capability_effect`；helper 只构造 serializable builder document。
- application workflow script 模块新增 typed builder document 解析合同，能从 `serde_json::Value` 解析 phase / parallel / agent / pipeline / function / local_effect / human_gate，并返回 pathful diagnostics。
- 聚焦测试覆盖 Rhai evaluator eval、workflow evaluator 中 Hook helper 不可用、typed builder document 解析和 Rhai syntax validate。

阶段产物已经成为 `ScriptCompiler` 的输入合同。Rhai adapter 只返回 builder document；application compiler 负责映射 common orchestration runtime IR。

### Phase 3：领域合同与资产边界（已完成）

1. 新增 `RunScriptArtifact` / `WorkflowScriptDefinition` value object 或 entity。
2. 明确 digest、revision、approval provenance、args schema、limits 和 capability summary 字段。
3. `RunScriptArtifact` 首版跟随 Lifecycle 保存；除非审批列表和跨 Lifecycle 查询已经需要，否则不拆独立 repository。
4. 全局注册为 `WorkflowScriptDefinition` 作为后续能力；首批只保留边界和 contract，不要求实现 projection。
5. 如需要持久化，新增 migration；字段不使用 `_json` / `_jsonb` 后缀。

完成状态：

- domain workflow value objects 新增 `RunScriptArtifact`、`WorkflowScriptDefinition`、`WorkflowScriptProvenance`、`WorkflowScriptCapabilitySummary`。
- `RunScriptArtifact` 表达 Lifecycle 内临时脚本草稿；`WorkflowScriptDefinition` 表达项目或库级可复用 definition asset。
- 首批没有新增 repository 或 migration，原因是 draft 归属 Lifecycle，当前交付只需要 value object、preflight contract 与 plan source ref；跨 Lifecycle 审批队列和全局 definition 列表出现真实读取粒度后再落仓储。
- JSON 字段保持业务名，例如 `args_schema`、`builder_document`、`capability_summary`，不使用存储实现后缀。

### Phase 4：ScriptCompiler（已完成）

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

完成状态：

- `agentdash-application::workflow::orchestration::ScriptCompiler` 消费 typed builder document，输出 `OrchestrationPlanSnapshot`。
- compiler 覆盖 phase、log、agent、parallel、pipeline、function、local_effect、human_gate、args root input binding、state exchange、capability summary 和 deterministic digest。
- `log()` 首批作为 metadata-only journal marker 编译，返回 warning diagnostic，不生成 executor node。
- phase node 是 metadata-only runtime container；activation 时标记为 `Skipped`，避免阻塞 terminal 汇总。
- root args 通过 plan metadata 中的 `root_input_bindings` 在 runtime activation 时物化到 entry node inputs。

### Phase 5：审批与启动 API（已完成 preflight surface）

1. 新增 draft create / preflight / approve / launch API。
2. API 返回 source、diagnostics、plan preview、capability summary。
3. approve 后创建 `OrchestrationInstance(role=dynamic_script)` 并交给 existing runtime drain。
4. 前端先做最小 preview，不做复杂可视化编辑器。

完成状态：

- `WorkflowScriptPreflightService` 组合 `WorkflowScriptEvaluator` 与 `WorkflowScriptCompiler`，返回 source、raw builder document、diagnostics、plan preview、plan snapshot 和 capability summary。
- API 新增 `POST /api/workflow-scripts/preflight`，只做审批前编译预览，不创建 draft、LifecycleRun、OrchestrationInstance 或 executor side effect。
- `agentdash-contracts` 新增 preflight request / response DTO 并生成前端 TS contract。
- `packages/app-web/src/services/workflow.ts` 新增 `preflightWorkflowScript()` 作为当前最小 preview 调用 surface。
- approve / launch 的持久化命令保持在 Lifecycle draft storage 任务中接入；当前任务已经验证 script frontend 可以产出正式 `OrchestrationPlanSnapshot`，并能进入 common runtime activation。

### Phase 6：保存为 workflow

1. 从 `RunScriptArtifact` 创建 `WorkflowScriptDefinition` revision。
2. 保存动作不修改已运行 `OrchestrationInstance.plan_snapshot`。
3. 资产安装/更新路径与 Shared Library contract 对齐。

完成状态：

- `WorkflowScriptDefinition` value object 已表达 project/library scope、revision、digest、source、args、limits、builder document、capability summary、compiled plan digest、installed source 和 provenance。
- 保存命令、全局列表、Shared Library projection 不在本任务拆仓储实现，原因是当前读写粒度还没有要求它脱离 Lifecycle / asset 管理路径。

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
cargo test -p agentdash-domain script_asset
cargo test -p agentdash-application script_compiler
cargo test -p agentdash-application orchestration
cargo check -p agentdash-api
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

## 收口判断

本任务已经完成“动态脚本作为 Orchestration Plan compiler frontend”的首批闭环：Rhai builder DSL、typed builder document、领域资产边界、preflight service/API、ScriptCompiler、runtime activation 和 contract TS surface 均已接入。后续若继续推进，应以 Lifecycle draft storage / approve-launch command / 保存为全局 workflow script definition 作为产品能力任务，而不是再改 compiler frontend 的基础模型。
