# WorkflowGraph 编译器实施计划

## 状态

暂不启动实现。本任务刻意停在规划状态，等 Orchestration 领域合同实现后，再一起评审编译器设计。

## 上下文顺序

实现代理必须读取：

1. 本任务 `prd.md`、`design.md`、`implement.md`。
2. `.trellis/tasks/06-06-orchestration-domain-contract` artifacts 和最终实现 diff。
3. 父任务编译器研究：`.trellis/tasks/06-06-dynamic-workflow-lifecycle-research/research/workflow-graph-compiler-plan.md`。
4. `implement.jsonl` 中列出的当前代码事实和 specs。

## 建议实施步骤

1. 重新打开最终 domain contract 类型名，并按实际合同调整本任务计划。
2. 评审后选择编译器模块位置：
   - 若只依赖 domain types，放在 domain pure service；
   - 若需要 source metadata 或 application preflight，放在 application compiler。
3. 定义 compile input / output 和 diagnostics。
4. 实现稳定 node id 与 source path 映射。
5. 将 Activity executor specs 映射到 plan executor specs。
6. 将 completion policies 映射到 result contracts。
7. 将 transitions 映射到 activation rules 与 artifact exchange rules。
8. 保留 join、iteration、artifact alias、traversal limits。
9. 添加正向与反向 fixtures。
10. 添加 deterministic snapshot / digest roundtrip tests。

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

## 停止条件

如果出现以下情况，停止并询问：

- domain contract 类型名与本任务假设明显不一致；
- 编译器需要新增 runtime 或 scheduler 行为；
- 编译器需要 generated DTO 或 frontend mapper 变更；
- graph validation 与 compiler strictness 冲突，并会改变用户可见 graph editor 语义。
