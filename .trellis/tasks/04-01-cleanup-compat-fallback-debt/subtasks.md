# 子任务追踪

## 总体策略

按“先消除错误掩盖，再收协议双轨，最后收执行器与开发链路”的顺序推进。

## 子任务清单

### ST-01 归属与 workspace 猜测收口

状态：`done`

目标：

- inline container 持久化不再跨 story / project 猜测归属
- address space 不再兜底选择“项目下第一个 workspace”
- owner / workspace 解析失败时暴露显式错误或显式空结果

涉及范围：

- `crates/agentdash-application/src/address_space/mount.rs`
- `crates/agentdash-application/src/address_space/inline_persistence.rs`
- `crates/agentdash-api/src/routes/address_spaces.rs`

### ST-02 持久化层静默默认值清理

状态：`doing`

目标：

- repository 读取坏 JSON / 坏枚举 / 坏时间时不再补业务默认值
- 明确哪些场景返回 `DomainError`

### ST-03 旧协议与旧路由清理

状态：`doing`

目标：

- 删除旧 project-agent / session 主路径
- 删除前端旧 task status / execution mode 映射
- 删除裸 `SessionNotification` / 传输降级兼容

### ST-04 执行器与 provider fallback 清理

状态：`todo`

目标：

- 删除 provider/model/default bridge 猜测性回退
- 删除伪造 discovery provider/model
- 逐步把 structured prompt 收敛为唯一真相

### ST-05 workflow 与 schema runtime migration 清理

状态：`todo`

目标：

- 删除 runtime legacy workflow contract 迁移
- 删除启动时 schema 自修复和补列逻辑

### ST-06 开发基础设施清理

状态：`todo`

目标：

- 去掉按进程名/端口暴力清场
- 收敛 embedded PostgreSQL 生命周期
- 统一 ready check / retry 脚本

## 当前批次

当前推进：`ST-03`

已完成：

- inline mount 现在会携带明确的 owner scope 元信息
- inline persistence 不再在 story scope 下回退写 project container
- address space 不再兜底选择“项目下第一个 workspace”
- 前端主链路已切到 `agent-links` 命名空间，不再走旧 `/projects/{id}/agents/...` 主路径
- 前端已删除旧 task status 兼容映射（`queued/succeeded/skipped/cancelled`）
- 前端 Task mapper 已改为对未知 `status/execution_mode` 显式报错
- executor 本地持久化已删除 `reasoningId` 老格式识别分支
- streamTransport 已删除裸 `SessionNotification` 兼容
- streamTransport 默认不再从 NDJSON 自动降级到 SSE，仅保留显式 `sse` 开关
- useAcpStream 已删除“异常累计 chunk 重复”专用去重补丁与对应测试
- Project / Story session info 已要求响应显式返回 `binding_id`，不再用请求参数回填
- Project agent session / open summary mapper 已要求显式 `binding_id`，不再静默映射为空串
- system event 渲染已收敛为显式白名单，不再按 `severity` 对未知事件兜底放行
- 项目会话列表 mapper 已改为对未知 `execution_status/owner_type` 显式报错
- project-session 上下文构建已改为严格失败，不再把坏 binding label / session meta / workspace 错误吞成空上下文
- project_agents 已改为在 agent link 指向缺失 agent 时直接报错，不再静默跳过脏数据
- open_project_agent_session 已删除基于 `summary.session` 的 `unwrap_or_default` 兜底
- resolve_agent_default_lifecycle 已删除旧 agent_key 解析岔路，只保留按 link agent id 查询
- Postgres `agent_repository` 已改为对 `base_config/config_override` 坏 JSON 显式报错，不再吞成默认对象
- Postgres `task_repository` 已改为对坏 `workspace_id/agent_binding/artifacts` 和未知 `status/execution_mode` 显式报错
- Postgres/sqlite `session_repository` 已改为对坏 JSON / 坏 event seq / 缺失 `visible_canvas_mount_ids_json` 直接失败
- SessionStore / memory persistence / session event stream 已改为对坏 JSON、坏 `last-event-id`、缺 session 直接失败
- `SessionOwnerType` 已删除 loose parse，API / service / repository 全链路改为严格解析
- tool execution artifact 已改为对非对象 content 和序列化失败直接报错
- 前端 `currentUser/session/workflow` mapper 已改为对未知 `auth_mode` / session status / workflow 枚举显式报错

下一步：

- 继续删除仍留在代码中的旧 project-agent 兼容痕迹
- 继续处理后端 preset MCP / legacy 路由兼容点
- 继续推进 ST-02：清理 project/story/workspace repository 层坏 JSON / 坏枚举 / 坏时间默认值
- 继续推进前端服务层 strict mapper，去掉把缺字段补空串 / 时间戳补当前时间的兜底

完成标准：

- 前端主链路不再使用旧 project-agent / session 路径
- 前端不再维护旧 task status 兼容映射
- 前端流传输主路径不再兼容裸 `SessionNotification`
- 默认流传输不再自动从 NDJSON 降级到 SSE
