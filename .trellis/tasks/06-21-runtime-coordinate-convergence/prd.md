# Runtime Coordinate 收敛

## Goal

统一 AgentRun 当前执行链路、RuntimeSessionExecutionAnchor selection、SubjectExecutionView history 与 AgentRun resource surface 坐标。后续 workspace、cancel、mailbox、SubjectExecutionView 和 resource browser 必须从同一 delivery binding / selection policy 读取执行目标。

## Decisions

- AgentRun 的 delivery runtime selection 必须全系统统一。
- AgentRun 应持有或可唯一解析 current delivery binding，表达当前运行链路。
- `RuntimeSessionExecutionAnchor` 继续作为 runtime trace 到 control-plane 的 backlink，但不承担业务层“当前执行目标”选择策略。
- repository raw latest 只允许表达底层排序查询，不表达 `current`、`primary`、`latest delivery` 等业务语义。

## Scope

- AgentRun current delivery binding / selection service。
- `RuntimeSessionExecutionAnchor` latest / primary / current-frame / run-scoped policy。
- AgentRun workspace、cancel、mailbox、SubjectExecutionView 的统一消费。
- SubjectExecutionView execution history。
- AgentRun resource surface 的 current frame / anchor launch frame 坐标表达。

## Out Of Scope

- 不重写 RuntimeSession event/feed 存储模型。
- 不把 RuntimeSession trace 细节复制到 AgentRun；AgentRun 只锚定控制面当前运行链路。
- 不处理 permission/capability exposure 事实源；该部分归 `06-21-capability-exposure-fact-convergence`。

## Acceptance Criteria

- [ ] `design.md` 定义 delivery binding / selection policy 的输入、输出、owner、失败语义。
- [ ] `work-items/index.md` 覆盖 D02、D03、D12、D15 及相关 P0/P1 backlog。
- [ ] workspace、cancel、mailbox、SubjectExecutionView 后续实现任务不再各自拼接 delivery target。
- [ ] repository raw latest API 有明确命名或使用边界。
- [ ] SubjectExecutionView history 与 latest 派生关系明确。

