# Session refactor batch 7 final convergence

## Goal

完成本轮 session launch/runtime 重构的最终代码收口：在 Batch 0-6 已完成的 owner / launch / runtime / terminal effect / pending command 迁移基础上，继续处理父任务原始 Batch 7 遗留的代码边界，而不是只做文档归档。

## Requirements

- 收紧 `working_dir` 路径策略：拒绝绝对路径、`..`、Windows prefix/root 越界输入，并保留合法相对路径投影。
- 清理 legacy `pending_capability_state_transitions_json` 持久化映射，使 runtime command store 成为唯一 pending runtime command 存储主线。
- 将 `SessionPersistence` 的 meta / event / terminal outbox / runtime command projection 边界拆成可依赖的 store 接口，避免继续把所有能力写成一个无差别大 trait。
- 收窄 AppState / SessionHub 初始化中的延迟注入：ready state 返回前必须完成 prompt augmenter、audit bus、terminal callback、tool provider 等必要绑定；文档和类型上不得再把未 ready 状态暴露为正常运行态。
- 将 `SessionHub` 剩余 public surface 明确分流到 core / runtime / effects / pending store 边界；如果仍保留 public shell，必须是迁移壳且不得新增业务判断。
- 运行最终验证矩阵，至少覆盖 application/api/infrastructure/local 的编译与核心 session tests。
- 父任务记录最终完成状态、已完成代码收口和真实剩余风险。

## Acceptance Criteria

- [ ] `resolve_working_dir` 不再接受绝对路径、parent segment 或 root/prefix 越界输入，并有单测覆盖。
- [ ] app / api / local / infrastructure 中不再读写 legacy `pending_capability_state_transitions_json` 字段。
- [ ] session persistence 至少暴露独立 meta/event/effect/runtime-command store 边界，调用方可以按能力依赖。
- [ ] AppState 构造返回前完成必要 session 依赖绑定；不再依赖返回后的外部补洞。
- [ ] `SessionHub` public shell 不再承载新增业务分支，剩余迁移壳边界在 spec 与 task notes 中如实记录。
- [ ] 最终验证矩阵通过。
- [ ] 提交历史按业务边界整理并 force-push 到当前 PR。
