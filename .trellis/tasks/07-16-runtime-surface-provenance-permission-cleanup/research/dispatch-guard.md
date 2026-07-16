# 执行派发与结果审查原则

本任务允许实现者根据完整调用链调整必要的邻接代码。主会话不以机械文件范围代替架构判断，而是对最终结果形态负责。

## 目标形态

- Runtime Surface adoption 只有一套 revision/digest/operation 事实，不依赖可选 Workflow 展示来源。
- Permission 只有一个 AgentRun 产品判断入口；当前默认允许。
- RuntimeInteraction 保留动态审批能力，但不与长期 Grant 混成同一状态机。
- 当前独立 PermissionGrant、permission_policy 及其 Surface/VFS/Hook/API/UI 旁路被清除。
- 未来 Grant 的 ownership 固定在 `LifecycleRun`，只经 AgentRun 暴露；本任务不提前实现。
- 完整产品链必须跑通：Agent 能创建 Canvas、写入或绘制内容、更新数据并展示，Surface adoption 后可以继续当前执行与后续对话。

## 实现判断

- 可以修改派发切片之外的邻接代码，但必须能说明它是完整链路的必要组成，而非顺手重构。
- 优先复用现有 canonical Runtime、AgentRun facade、RuntimeInteraction、Canvas 与 Surface 机制。
- 新增抽象必须对应当前 producer、consumer、owner、正确性不变量和失败测试。
- 发现规划假设与真实代码冲突时，以完整数据流证据为准，并向主会话说明建议调整。
- 不以“最少文件修改”为目标；以最少事实源、最少重复机制和可运行闭环为目标。

## 过度设计嫌疑

以下情况必须由主会话重点审查：

- 为未来 Grant 提前增加字段、状态机、repository、API、UI、scope taxonomy 或 TTL。
- 同一授权事实再次进入 AgentFrame、Business Surface、VFS、Hook 或 Driver config。
- 为展示 metadata 建立新的全局 provenance taxonomy。
- 为解决局部编译问题复制第二套 mapper、policy、read model 或恢复路径。
- 新增模块无法指出删除后会失败的当前测试。

## 交付说明

每个实现切片应说明：

- 完整数据流中修复了哪个断点；
- 哪个对象是最终事实源；
- 新增或保留的复杂度服务什么当前不变量；
- 运行了哪些单元、集成或产品流程验证；
- 仍需主会话统一处理的跨切片问题。
