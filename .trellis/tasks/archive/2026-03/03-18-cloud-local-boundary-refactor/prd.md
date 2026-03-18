# 云端/本机后端边界重构规划

## 目标

将 AgentDashboard 的双后端架构从“设计上分离、实现上混合”收敛为“职责清晰、运行路径唯一”的状态：

- 云端后端只负责数据、编排、云端原生 Agent、WebSocket 中继服务和 API 聚合
- 本机后端只负责第三方 Agent 执行、工作空间文件访问、Git 检测、PiAgent 工具执行
- 移除云端对本地文件系统和本地执行环境的隐式依赖
- 明确 workspace 路由键，避免再通过路径前缀猜测目标 backend

## 背景

项目规范和设计文档已经明确给出了双后端边界：

- 云端是数据中枢和用户入口
- 本机提供文件系统和第三方 Agent 执行能力
- 云端代码不应直接访问本地文件系统
- 运行时路由应基于 `Task.workspace_id -> Workspace.backend_id`

但当前代码实现仍然保留了较强的单机模式遗产，导致“远程 backend 只是优先路径，而不是唯一合法路径”。

## 现状结论

### 1. 云端仍初始化本地执行能力

`agentdash-api` 启动时会基于当前工作目录初始化：

- `VibeKanbanExecutorsConnector`
- `PiAgentConnector`
- `ExecutorHub`

这意味着云端二进制仍具备完整的本地 Agent 执行能力，而不是纯粹的编排与中继节点。

### 2. 任务执行存在远程失败回退

Task 执行链路会先检查目标 backend 是否在线：

- 在线：通过 relay 下发到本机
- 不在线：直接调用云端自己的 `executor_hub.start_prompt()`

这使得同一个 Task 会因为运行时在线状态而落到不同机器上执行，破坏了部署模型与权限边界。

### 3. 工作空间文件接口仍可回退到云端本地文件系统

`workspace-files` 相关接口的行为是：

- 若 `container_ref` 能映射到在线 backend，则 relay 到本机
- 否则回退到云端自身 `executor_hub.workspace_root()`

问题在于：

- 返回的文件根目录可能不是目标 workspace
- 云端会直接访问自身宿主机文件系统
- API 语义依赖部署位置，不是稳定的业务契约

### 4. Workspace 管理面仍直接操作云端宿主机目录

以下操作仍在 cloud 侧直接运行：

- `container_ref` 规范化
- 目录存在性检查
- Git 仓库探测
- 目录选择器弹窗

这在本地开发时“看起来可用”，但在真正的云端部署模型下并不成立。

### 5. 路由键未收敛，仍依赖路径推断

规范中要求路由基于 `Workspace.backend_id`，但当前 `Workspace` 实体和表结构中没有该字段。

因此代码只能通过：

- `workspace.container_ref`
- `backend.accessible_roots`

去做“路径前缀匹配”式的 backend 推断。该方式不稳定，也无法表达明确归属。

### 6. 路径契约仍残留单机模式

当前执行上下文里仍频繁把 `workspace.container_ref` 当作：

- `working_dir`
- prompt 注入中的“工作目录”
- 声明式上下文解析的本地根目录

这让云端继续对本地绝对路径有语义依赖，不利于后续彻底收敛职责。

### 7. Relay 安全收口仍不完整

WebSocket 接入点当前读取了 `token`，但注册链路中没有完成真正的鉴权校验。

这不是本次边界问题的唯一核心，但属于同一层架构清理中的必要补项。

## 核心问题定义

当前系统的真实问题不是“还有几个接口没迁走”，而是：

1. 云端进程仍保留了一整套单机模式运行内核
2. 远程 backend 只是优先路径，不是强约束边界
3. Workspace 的归属和路由键没有在领域模型中被正式表达
4. 文件访问、执行、Git 探测、目录选择等能力没有全部下沉到本机后端

## 目标态

### 职责边界

#### 云端后端负责

- Project / Workspace / Story / Task / Backend 等业务数据
- 任务编排、状态机、MCP 注入、上下文拼装
- 云端原生 PiAgent AgentLoop
- Relay WebSocket 服务端
- 面向前端的 API 聚合与事件转发

#### 本机后端负责

- 第三方 Agent 执行
- 工作空间文件浏览/读取/写入
- Git 仓库探测
- 目录选择与本地路径解析
- PiAgent tool call 的本地落地执行

### 运行约束

- 云端不得再 fallback 到本地执行第三方 Agent
- 云端不得直接读取/遍历/探测 `Workspace.container_ref` 指向的本地目录
- 任意与 workspace 物理文件相关的能力，必须路由到 `Workspace.backend_id`
- backend 不在线时，应返回明确错误，而不是静默回退

### 契约约束

- `Workspace.backend_id` 成为正式字段
- `container_ref` 仅表示“该 backend 可理解的本地目录定位符”
- `working_dir` 应使用相对 workspace 根目录的路径
- relay 命令中的 `workspace_root` 仅供 local backend 使用

## 非目标

- 不考虑兼容旧部署方式
- 不为当前预研阶段设计复杂迁移方案
- 不在本任务中完成所有实现代码，只定义清晰改造路径与验收标准

## 分阶段重构方案

### Phase 0：收口设计与契约

目标：先把“应该怎么做”写死，避免边改边漂。

工作项：

- 为 `Workspace` 增加 `backend_id` 字段，并明确其含义是物理归属 backend
- 更新相关设计文档和后端规范，统一运行时路由模型
- 明确 `container_ref`、`workspace_root`、`working_dir` 的职责和格式
- 明确云端 API 在 backend 不在线时的失败语义

产出：

- 更新后的领域模型与持久化结构定义
- 路由契约说明
- 错误语义说明

### Phase 1：切断云端本地执行 fallback

目标：先消除最危险的“执行位置漂移”。

工作项：

- 移除 cloud 正式路径中对本地第三方执行器的依赖
- Task 执行链路中，若目标 backend 不在线，则直接失败
- cancel / session overview / stream 等相关路径同步区分本地/远程来源
- 为云端原生 PiAgent 保留云端执行能力，但其工具调用仍必须走 relay

验收标准：

- 指向 remote backend 的 Task 在 backend 离线时返回显式错误
- cloud 不会因为 backend 离线而直接在自身机器上启动第三方 Agent

### Phase 2：将 workspace 物理能力全部下沉到 local backend

目标：云端不再直接解引用本地路径。

工作项：

- `workspace-files` 只做路由和聚合，不再直接读取云端本地文件
- `address-spaces/workspace_file` 改为依赖 local backend 返回条目
- `workspace_detect_git` 彻底走 relay
- `pick_directory` 能力迁移到 local backend，或在产品层明确只允许本机 UI 调用本机 API
- cloud 删除 `std::fs` / `tokio::fs` / `git2` / `rfd::FileDialog` 等与本地目录直接相关的正式依赖路径

验收标准：

- cloud 二进制在生产路径下不再直接访问 workspace 物理目录
- 任意 workspace 文件相关接口都必须具备明确 backend 归属

### Phase 3：收敛路径与上下文契约

目标：去掉云端对绝对本地路径的语义依赖。

工作项：

- 上下文构建中不再把 `container_ref` 当作可直接在云端使用的本地路径
- `working_dir` 全部改为相对路径
- prompt 注入中保留“目录语义”，不暴露无意义的云端宿主路径
- 声明式 source resolver 需要区分：
  - 云端可解析的资源
  - 只能由目标 backend 解析的 workspace 资源

验收标准：

- 关键执行链路中不再把绝对 `container_ref` 注入为 `working_dir`
- 与 workspace 相关的路径处理都显式依赖 backend 上下文

### Phase 4：补齐安全与观测

目标：让新边界可验证、可维护。

工作项：

- 补上 relay WebSocket token 校验
- 为 backend 离线、路径非法、backend 不匹配等场景定义错误码
- 增加集成测试，覆盖：
  - backend 离线不再 fallback
  - workspace 文件接口不再访问 cloud 本地目录
  - backend 路由依赖 `Workspace.backend_id`
- 记录日志字段，便于观察路由决策与失败原因

## 建议拆分的后续实现任务

### P0

- 建立 `Workspace.backend_id` 并完成所有运行时路由改造
- 移除 Task 执行 fallback

### P1

- `workspace-files` / `address-spaces` / `detect_git` 全量 relay 化
- 清理 cloud 内的本地文件访问正式路径

### P2

- 路径契约清理
- prompt / context / source resolver 收口
- relay 鉴权与观测补强

## 验收标准

- [ ] `Workspace.backend_id` 成为正式字段，运行时不再依赖路径前缀猜路由
- [ ] 云端不再在远程 backend 离线时本地执行第三方 Agent
- [ ] 云端不再直接读取或遍历 workspace 物理目录
- [ ] Git 探测、目录选择、workspace 文件能力全部归属本机后端
- [ ] `working_dir` 契约统一为相对路径
- [ ] relay 注册链路具备有效鉴权
- [ ] 关键链路具备集成测试覆盖

## 关键文件清单

- `.trellis/spec/backend/index.md`
- `docs/relay-protocol.md`
- `docs/modules/09-relay.md`
- `crates/agentdash-api/src/app_state.rs`
- `crates/agentdash-api/src/bootstrap/task_execution_gateway.rs`
- `crates/agentdash-api/src/routes/workspace_files.rs`
- `crates/agentdash-api/src/routes/workspaces.rs`
- `crates/agentdash-api/src/routes/address_spaces.rs`
- `crates/agentdash-api/src/task_agent_context.rs`
- `crates/agentdash-api/src/relay/registry.rs`
- `crates/agentdash-api/src/relay/ws_handler.rs`
- `crates/agentdash-local/src/main.rs`
- `crates/agentdash-local/src/ws_client.rs`
- `crates/agentdash-local/src/tool_executor.rs`

## 备注

该任务是“上位规划任务”，建议后续实际改造按阶段拆成 2 到 4 个实现任务，避免一个大任务同时改领域模型、relay 协议、API 路由和上下文系统，导致回归面失控。
