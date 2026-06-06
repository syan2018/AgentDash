# WorkflowGraph 编译器实施计划

## 状态

暂不启动实现。本任务刻意停在规划状态，等 Orchestration 领域合同实现后，再一起评审编译器设计。

## 上下文顺序

实现代理必须读取：

1. 本任务 `prd.md`、`design.md`、`implement.md`。
2. `.trellis/tasks/06-06-orchestration-domain-contract` artifacts 和最终实现 diff。
3. 父任务编译器研究：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/workflow-graph-compiler-plan.md`。
4. 父任务两份 Claude Workflow 资料副本与行为覆盖矩阵，尤其是 `flow` 作为过程控制、artifact / 变量作为状态交换的语义。
5. `implement.jsonl` 中列出的当前代码事实和 specs。

## 建议实施步骤

1. 重新打开最终 domain contract 类型名，并先修正 compiler 必须依赖的合同偏差：
   - `OrchestrationPlanSnapshot` 的内容身份使用 deterministic digest；
   - `OrchestrationInstance` / run / agent run 继续使用 UUID；
   - `PlanNodeKind::Activity` 不作为 graph compiler 默认输出。
2. 在 application 层新增纯 compiler 模块，例如 `crates/agentdash-application/src/workflow/orchestration/compiler.rs`，并从 `workflow/mod.rs` 暴露内部使用入口。
3. 定义 compile input / output 和 diagnostics。input 只接收 `WorkflowGraph`、source metadata、compile mode、target schema version；不读 repository。
4. 实现 canonical source normalization 与 digest 计算，保证相同 graph + compiler schema 得到相同 plan digest。
5. 实现稳定 node id 与 source path 映射，source activity key 写入 metadata / source ref，运行节点按 executor 输出 semantic kind。
6. 将 Activity executor specs 映射到 semantic plan nodes：
   - Agent -> `PlanNodeKind::AgentCall` + `ExecutorSpec::AgentProcedure`
   - ApiRequest -> `PlanNodeKind::Function` + `ExecutorSpec::Function`
   - BashExec -> `PlanNodeKind::LocalEffect` 或等价 typed effect executor
   - Human approval -> `PlanNodeKind::HumanGate` + `ExecutorSpec::Human`
7. 将 completion policies 映射到 result contracts，保留 HookGate / OpenEnded extension 语义。
8. 将 transitions 规范化为两个维度：
   - control dependency / condition / join / traversal limit；
   - state exchange / artifact binding / input materialization。
9. 保留 join、iteration、artifact alias、traversal limits，不因为旧 runtime 未执行而丢弃。
10. 添加正向与反向 fixtures。
11. 添加 deterministic snapshot / digest roundtrip tests。

## 验证命令

具体命令等 domain contract 落地后定稿。预期最小集合：

```powershell
cargo test -p agentdash-domain workflow_graph_compiler
cargo test -p agentdash-domain orchestration
git diff --check
```

如果编译器放在 application：

```powershell
cargo test -p agentdash-application workflow_graph_compiler
```

当前计划推荐 application，因此 implementation review 后的默认命令应以后者为主；domain 测试只覆盖 IR/value object 和必要的合同修正。

## 停止条件

如果出现以下情况，停止并询问：

- domain contract 类型名与本任务假设明显不一致；
- 编译器需要新增 runtime 或 scheduler 行为；
- 编译器需要 generated DTO 或 frontend mapper 变更；
- graph validation 与 compiler strictness 冲突，并会改变用户可见 graph editor 语义；
- 实现需要把 graph 编译成脚本或脚本 AST 才能推进；
- 发现旧 flow/artifact edge 语义无法无损规范化为控制流 + 状态交换两个维度。
