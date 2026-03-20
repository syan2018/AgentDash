# 前端配置上下文传递规则

## 目标

把 `Project / Story / Task / Session Runtime` 之间的上下文承接规则固定成一套统一口径，供前端展示、编辑和调试使用。

## 外层到内层的规则

### 1. Project 是默认配置层

Project 提供整条链路的默认值：

- `config.default_agent_type`
- `config.context_containers`
- `config.mount_policy`
- `config.session_composition`

这些字段定义的是“如果 Story / Task 没有进一步覆盖，系统默认怎么运行”。

### 2. Story 是覆盖与补充层

Story 不负责重新定义全部规则，而是在 Project 默认之上做“增量补充”和“局部覆盖”：

- `context.context_containers`
  - 追加 Story 自己的虚拟容器
- `context.disabled_container_ids`
  - 从 Project 默认容器中禁用指定容器
- `context.mount_policy_override`
  - 覆盖 Project 默认挂载策略
- `context.session_composition_override`
  - 对 persona / workflow / required_context_blocks 做非空覆盖

### 3. Task 决定 Agent 身份解析

Task 主要影响“当前会话到底以什么 Agent 身份运行”，解析顺序为：

1. `task.agent_binding.agent_type`
2. `task.agent_binding.preset_name`
3. `project.config.default_agent_type`

这个结果会继续影响：

- 哪些容器满足 `allowed_agent_types`
- 当前会话是否使用 native address space
- 运行时工具集与路径规则如何展示

### 4. Session Runtime 是最终生效层

当会话真正启动时，系统才会把前面几层配置收束成最终运行结果：

- `effective mount policy`
  - `story.mount_policy_override ?? project.mount_policy`
- `effective session composition`
  - `project.session_composition` 先作为默认值
  - 再用 `story.session_composition_override` 的非空字段覆盖
- `effective containers`
  - `project.context_containers`
  - 减去 `story.disabled_container_ids`
  - 再叠加 `story.context_containers`
  - 然后按 `exposure` 和 `allowed_agent_types` 过滤
- `runtime address_space`
  - 只在 native address space 场景下生成
- `tool visibility / runtime policy`
  - 由最终 address space + MCP 注入共同决定

## 前端展示建议

前端不要把这些层混在一起展示，而应该显式拆开：

1. `Project 默认`
2. `Story 覆盖`
3. `当前生效`
4. `Session Runtime`

其中：

- `Project 默认` 用来解释“系统起点是什么”
- `Story 覆盖` 用来解释“本 Story 改了什么”
- `当前生效` 用来解释“合并后的正式配置是什么”
- `Session Runtime` 用来解释“Agent 这一次真正能看到什么、能用什么工具”

## 当前实现对齐点

- `GET /tasks/:id/session`
  - 返回 `context_snapshot`
  - 返回 `address_space`
- `SessionPage`
  - 按 `Project 默认 → Story 覆盖 → 当前生效 → Runtime` 展示
  - 显示 agent 解析来源、工具可见性、运行策略和 mounts

## 仍待补齐

- Story / Project 编辑页还需要更完整的结构化编辑入口：
  - provider 细节
  - exposure 细节
  - required_context_blocks 编辑
  - disabled_container_ids / mount_policy_override 的正式编辑入口
- Story Session（非 Task Session）也需要同等粒度的上下文快照
