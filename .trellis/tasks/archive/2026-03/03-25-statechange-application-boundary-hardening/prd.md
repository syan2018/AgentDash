# 后端状态一致性与应用层收口

## 目标

- 关键状态变更必须与 `StateChange` 记录保持一致。
- project/story/task 的关键用例从 API route 收回 application service。
- 为删除、克隆等多步操作建立明确事务边界。

## 非目标

- 不在本任务内完成所有领域模型类型化。
- 不追求一次性重写所有 route，只覆盖高风险主路径。

## 验收标准

- 关键变更路径不再出现 `append_change(...).await.ok()` 这类吞错写法。
- `delete_project`、`clone_project`、高风险 story/task 更新流程具备原子性或显式补偿策略。
- route 文件职责明显变薄，新增测试覆盖关键失败分支。
