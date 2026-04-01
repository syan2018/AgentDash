# Live Canvas：动态前端运行时

## 背景

AgentDash 当前的 Agent 执行产出主要以文本消息和结构化 JSON 形式呈现。对于需要可视化、交互式表格、统计图表等场景（如关卡策划的物件布设统计），缺乏让 Agent 动态生成并运行前端代码的能力。

## Goal

让 Agent 能在平台内创建和维护可直接运行的前端代码资产（Canvas），用于向用户呈现可视化、交互式数据表、表单等。Canvas 作为 Mount 体系中的一等资产，Agent 可通过其它 mount 中的文件资产（如 lifecycle_vfs 中的工具调用记录）驱动 Canvas 的数据更新。

## 核心设计约束

- **一个 Canvas = 一个 Mount**：Canvas 是独立的 mount 空间，内部为多文件结构
- **Project 级资产**：Canvas 挂在 Project 下，通常由 Project Agent 创建
- **数据注入走文件引用**：数据以其它 mount 中的文件路径传入沙箱，而非裸 JSON；典型来源是 `lifecycle_vfs` 下 `active/artifacts/{id}` 等已有文件资产
- **沙箱隔离**：Canvas 代码在隔离环境中运行，不可访问宿主页面
- **预装库 Project 级可配置**：不同项目可定制沙箱中可用的前端库（游戏项目可能要 Three.js，数据项目要 ECharts）
- **Present 信号走 ACP 系统事件**：Agent 通过 `canvas_presented` 系统事件通知前端展示 Canvas

---

## 现有架构集成点

### Mount / Address Space

Canvas 的后端存储和访问复用统一 Mount 体系，新增 `canvas_fs` provider：

- **`MountProvider` trait**（`crates/agentdash-spi/src/mount.rs`）：定义 `read_text`、`write_text`、`list`、`search_text`、`apply_patch` 等能力。`canvas_fs` 需实现此 trait。
- **`MountProviderRegistry`**（`crates/agentdash-application/src/address_space/provider.rs`）：通过 `register()` 注册 provider。当前 `MountProviderRegistryBuilder::with_builtins()` 只注册了 `inline_fs` 和 `lifecycle_vfs`；`relay_fs` 由 API 层追加。`canvas_fs` 可按同样方式注册。
- **FS 工具统一调度**：`FsReadTool` / `FsWriteTool` / `FsListTool` 等（`crates/agentdash-application/src/address_space/tools/fs.rs`）通过 `RelayAddressSpaceService` 按 `mount.provider` 字段分派到对应 `MountProvider`，不需要为 Canvas 单独创建一套读写工具。Agent 可直接用 `fs_write(path="canvas-xxx://src/main.tsx", content=...)` 维护文件。
- **编辑能力注意事项**：`RelayAddressSpaceService::apply_patch()` 会先根据 provider 的 `MountEditCapabilities` 组合执行 create/delete/rename；若能力不足才回退 provider 原生 `apply_patch`。因此 `canvas_fs` 若希望完整支持 `fs_apply_patch` 的 Add/Delete/Move，最好显式声明 `create/delete/rename` 能力，而不是只做最小 `write_text`。
- **动态挂载的现成范式**：当前 Task 运行时已在 `crates/agentdash-application/src/task/session_runtime_inputs.rs` 中先调用 `build_address_space()`，再额外 `push(build_lifecycle_mount(...))`。Canvas mount 最适合沿用这一路线：以派生 Address Space 为基础，再按当前 session 追加 Canvas mounts，而不是强行塞回 `context_containers`。
- **参考实现**：
  - `InlineFsMountProvider`（`provider_inline.rs`）：从 `mount.metadata.files` 读文件，写入走 `InlineContentOverlay` + `DbInlineContentPersister` 持久化到 Project/Story 配置
  - `LifecycleMountProvider`（`provider_lifecycle.rs`）：虚拟 FS，路径路由到 `LifecycleRunRepository`（`active/steps/{key}`、`active/artifacts/{id}`、`active/log` 等），只读
- **Mount 常量**：`mount.rs` 中的 `PROVIDER_INLINE_FS` / `PROVIDER_LIFECYCLE_VFS` / `PROVIDER_RELAY_FS`，需新增 `PROVIDER_CANVAS_FS`

基于当前代码结构，**不建议**把 Canvas 实体直接复用为 `ProjectConfig.context_containers`：

- `context_containers` 当前是 Project/Story 配置的一部分，适合声明式上下文容器，不适合承载独立项目资产的生命周期和元数据
- `build_derived_address_space()` 只会从 Project/Story 配置派生 mount；若 Canvas 作为独立 DB 实体存在，更自然的做法是“先派生基础 mount，再按 session 追加 Canvas mount”
- `get_session_context()` 每次会按 owner 重新构建上下文；因此若希望刷新后仍能看见 Canvas mount，Canvas 与其绑定关系必须是**可持久化重建**的，而不能只是内存态临时注入

### Agent 工具注册

- **`RelayRuntimeToolProvider::build_tools()`**（`crates/agentdash-application/src/address_space/tools/provider.rs`）：根据 `ExecutionContext.flow_capabilities` 按需追加工具。FS 工具始终加入；`companion_dispatch`、`resolve_hook_action`、`workflow_artifact` 等按 capability 开关追加。
- Canvas 专用工具（如 `create_canvas`、`present_canvas`、`inject_canvas_data`）应同样受 capability 控制（如 `flow_capabilities.canvas`），在 `build_tools` 中条件追加。
- **无需** `update_canvas_file` 等冗余工具——Canvas mount 挂载后，Agent 直接用已有 `fs_write` / `fs_apply_patch` 维护文件。

基于当前实现，Canvas 工具接入至少还要同步修改三处：

- **`FlowCapabilities`**（`crates/agentdash-spi/src/connector.rs`）：当前只有 `workflow_artifact` / `companion_dispatch` / `companion_complete` / `resolve_hook_action` 四个布尔位，需新增 Canvas 相关 capability
- **Session 请求增强链路**（`crates/agentdash-api/src/routes/acp_sessions.rs`）：Project / Story / Task 三条 prompt request builder 目前都会显式写入 `FlowCapabilities`，需要同步补上 Canvas 开关
- **`session_plan.rs` 的工具可见性摘要**（`crates/agentdash-application/src/session_plan.rs`）：当前 prompt 里显示的“可见工具”是按 owner type 手工拼出的；如果新增 Canvas 工具，不同步更新这里，Agent 看到的工具面摘要会与真实注入的工具集不一致

### 系统事件（Present 信号）

当前项目**没有** `SystemEvent` 类型——系统事件通过 `SessionNotification` + `SessionUpdate::SessionInfoUpdate` + `_meta.agentdash` 元数据表达：

- **Rust 侧**：构造 `AgentDashEventV1::new("canvas_presented")`，填入 `data`（canvas_id、mount_id 等），通过 `merge_agentdash_meta` 注入 `SessionInfoUpdate.meta`，再 `SessionHub::inject_notification` 推送到会话流
- **参考**：`companion.rs` 的 `build_companion_event_notification`、`hook_action.rs` 的 `build_hook_action_resolved_notification`
- **共用元数据模型**：`AgentDashEventV1` 本身是 free-form `type/code/message/data` 结构（`crates/agentdash-acp-meta/src/lib.rs`），不需要为 Canvas 额外发明事件协议层
- **前端识别**：
  - `AcpSystemEventGuard.ts`：`VISIBLE_SYSTEM_EVENT_TYPES` 白名单中加入 `"canvas_presented"`
  - `AcpSystemEventCard.tsx`：`EVENT_TYPE_LABELS` / `EVENT_TYPE_DEFAULT_MESSAGES` 中增加对应展示
  - `SessionPage.tsx`：`handleSystemEvent` switch 中增加 `"canvas_presented"` 分支，触发 Canvas Panel 打开

补充说明：

- `SessionChatView` 的 `onSystemEvent` 签名已经是 `(eventType, update)`，当前 `SessionPage` 只用了 `eventType`。Canvas 场景下可以直接消费第二个参数里的 `_meta.agentdash.event.data`，不需要修改 `SessionChatView` 本身
- 如果后续还需要 `canvas_data_injected`、`canvas_refresh_requested` 等事件，建议都沿用同一套 AgentDash Meta 表达，避免在 ACP 流里再并行维护第二种 payload 结构

### Session 页布局

当前 `SessionPage.tsx` 布局为单列纵向：

```
header (border-b, shrink-0)
  ├ [CHAT] 标签 + 会话标题
  └ 返回/复制/新会话 按钮
optional context panel (可折叠, max-h-[42vh])
  ├ ProjectSessionContextPanel (项目级会话)
  └ StorySessionContextPanel (story/task 级会话)
SessionChatView (flex-1, overflow-hidden)
  └ 完整的 ACP 流 + 输入框
```

Canvas Panel 集成需要将当前单列改为双列布局，或在 `SessionChatView` 旁新增侧边面板。备选方案：

- **侧边栏**：`flex flex-row`，SessionChatView 占主列，Canvas Panel 占侧栏（可拖拽宽度）
- **弹出抽屉**：Canvas Panel 作为右侧抽屉覆盖，不影响现有布局
- **内嵌到 context panel 区域**：复用可折叠面板壳，但高度受限

结合当前代码，更建议首版走 **右侧抽屉 / 侧栏**，理由是：

- `SessionPage` 现在是 `header -> context panel -> SessionChatView` 的纵向堆叠，改成“聊天区 + Canvas 区”二栏比把 Canvas 强塞进 context panel 更符合现有职责分层
- `StorySessionPanel` / `TaskAgentSessionPanel` 也都直接复用 `SessionChatView`，因此 Canvas UI 最适合挂在页面级容器，而不是侵入 `SessionChatView`
- 首版若只在独立 `SessionPage` 落地 Canvas Panel，变更面可控；Story / Task 内嵌面板可以先不做

### 数据注入机制

Canvas 代码需要消费外部数据。数据来源是其它 mount 中的文件，典型场景：

- Agent 在 `lifecycle_vfs` mount 下执行工作流，产生结构化产物文件（如 `lifecycle://active/artifacts/{id}`）
- Agent 调用 `inject_canvas_data` 工具，传入**文件引用**（`mount_id://path` 格式）而非裸数据；后端仅持久化绑定关系
- Canvas Panel 打开或刷新时，由后端按绑定关系读取源文件，构造当前 Canvas 的**运行时文件快照**（Canvas 自身文件 + 注入的数据文件）
- 前端将该快照装载到 iframe 沙箱；数据文件在沙箱内以约定路径暴露（如 `/bindings/<alias>.json`）
- 如需增量刷新，优先传输“变更后的文件集合”而非新的裸 JSON 事件载荷；`canvas_data_injected` 事件只承担“有绑定/数据已更新”的通知职责

结合现有接口分工，推荐把“运行时文件快照”做成 **独立 Canvas API**，而不是塞进现有 `/sessions/{id}/context`：

- `get_session_context()` 当前职责是返回 owner 级上下文快照（workspace / address_space / context_snapshot），不适合承载体积更大、更新更频繁的 Canvas 文件集合
- Canvas runtime 快照天然是“按 canvas_id + 当前 session 解析绑定后得到”的资源，适合独立为 `GET /api/canvases/{id}/runtime-snapshot?session_id=...`
- 这样 `canvas_presented` 事件只需告诉前端“打开哪个 canvas”，真正文件内容仍按需拉取，避免把大量内容塞进流事件

---

## 当前推荐实现方向

结合当前目标边界，推荐 **优先自研极简 Canvas Runtime**，而不是将 Sandpack 作为正式方案主干。

### 选择判断

- 当前需求重点是“平台内可维护的可视化资产运行时”，而不是“面向开发者的在线代码 playground”
- 已确认 **不需要** 任意 npm 包即插即用、强编辑器体验、多框架广覆盖，这显著降低了自研成本
- Canvas 的核心模型已经天然接近“多文件 mount + 文件绑定 + 隔离 iframe”，与自研 runtime 更对齐
- 若桥接 Sandpack，短期能更快出样机，但会引入一层长期 adapter：mount 文件树、项目级白名单库、Present 事件、数据文件绑定都要翻译成 Sandpack 的 `template/files/dependencies` 心智模型

### 推荐结论

- **正式路线**：自研 `iframe sandbox` Canvas runtime
- **Sandpack 定位**：仅作为对照组 / 技术样机参考，不作为当前正式方案前提
- **MVP 原则**：优先支持受控白名单库、整页重载、文件级数据注入，不追求通用 bundler、在线 IDE、热模块替换

### 基于当前代码的进一步收敛

- **Canvas 作为独立项目资产**：新增独立 Canvas 实体 / repo / API，不复用 `context_containers`
- **Address Space 以“基础派生 + 运行时追加”实现**：参考 lifecycle mount 的追加模式，把 Canvas mount 放到 Task / Story / Project session 的最终 address space 里
- **Canvas 工具能力走现有 SPI**：文件编辑复用 `fs_*`；仅新增少量高层工具用于创建实体、绑定数据、发送 present 事件
- **Canvas 运行时快照走独立 API**：session stream 只发“展示 / 刷新”信号，不承载大块文件数据
- **首版 UI 落在独立 SessionPage**：先打通页面级 Canvas Panel，不急着侵入 Story / Task 的嵌入式会话面板

### 自研 Runtime 的最小分层

1. **Canvas 领域层**
   定义 Canvas 实体、入口文件、允许库白名单、数据绑定声明、展示状态。
2. **Mount / Snapshot 层**
   从 `canvas_fs` 与外部数据源 mount 读取文件，拼出运行时快照；这是平台真实的数据边界。
3. **Runtime 组装层**
   将快照编译/封装为可执行的 iframe 文档，注入 import map、启动脚本、错误上报桥。
4. **Sandbox 通信层**
   通过 `postMessage` 实现 `ready / render_ok / render_error / refresh_requested` 等协议。
5. **Session 展示层**
   Canvas Panel 负责打开、关闭、刷新、错误呈现、加载态和空态。

---

## 需要在实施前确认的事项

- [ ] **Runtime 形态确认**：以自研 `iframe sandbox` 为主线，先确认首版是否接受“整页重载 + 白名单 CDN/import map 库 + TypeScript/TSX 编译”的窄能力边界；Sandpack 仅保留为样机对照组
- [ ] **Canvas 领域实体设计**：独立 DB 表 / repo / service 的字段（project_id、title、entry_file、sandbox_config、bindings 等）；当前更倾向“独立资产 + 运行时追加 mount”，**不**复用 `ProjectConfig.context_containers`
- [ ] **canvas_fs provider 能力边界**：read + write + list 基本确定；是否需要 search（文件数量通常不多）；是否需要 `InlineContentOverlay` 类似的 session 内 write-through cache
- [ ] **Canvas mount 加入 Session mount table 的时机**：更推荐沿用 lifecycle mount 模式，在 owner-specific context builder / runtime inputs 中“先 build_address_space，再 push Canvas mounts”
- [ ] **前端 Canvas Panel 布局**：侧栏 vs 抽屉 vs 内嵌面板
- [ ] **数据绑定声明**：Canvas 如何声明它需要哪些外部文件作为数据源、绑定后在沙箱内映射成什么路径、更新时是整页刷新还是文件级 patch
- [ ] **受控库清单**：MVP 默认支持哪些库（建议 React / ReactDOM / ECharts / 基础表格库）；项目级白名单如何配置与校验
- [ ] **编译策略**：首版是否使用 `esbuild-wasm` 仅处理 TS/JSX + import map；CSS、图片等静态资源是否暂缓支持
- [ ] **Capability 扩展点**：`FlowCapabilities`、`acp_sessions` owner prompt builder、`session_plan` 工具摘要要一起改，避免“工具真实可用但 prompt 摘要不可见”的错配
- [ ] **SessionPage 事件消费**：`canvas_presented` 是否只负责打开面板，还是同时携带“默认 tab / 标题 / 建议尺寸”等 UI hint

## 优先场景

- 交互式数据表（可排序/筛选/导出）
- 数据可视化图表（ECharts/D3 等）
- 交互式表单/配置器（Agent 生成、用户填写）
- 关卡策划场景：物件布设统计表、资源分布热力图、配置对比视图

## 非目标（MVP）

- Canvas 用户交互回传给 Agent（Hook 机制，留档后延后）
- Canvas 版本历史
- 组织级 Canvas 模板库与共享
- Canvas 间数据联动
- 导出为独立页面

## 技术备忘

### 沙箱方案对比

| 方案 | 多文件 | npm 依赖 | 安全隔离 | 嵌入成本 | 备注 |
|------|--------|---------|---------|---------|------|
| **Sandpack**（CodeSandbox） | ✅ 虚拟文件系统 | ✅ bundler 内置 | ✅ iframe | 中（React 组件） | 适合快速样机，但抽象偏 playground，长期会引入 adapter 层 |
| **自研 iframe runtime** | ✅ 运行时快照 | ❌ 受控白名单库 | ✅ sandbox attr | 中 | 更贴合 Canvas=Mount + 文件绑定模型，推荐正式路线 |
| **Renderize** | ❌ 单文件 | ❌ 预装固定 | ✅ srcdoc iframe | 极低 | 太受限，不满足多文件需求 |
| **WebContainer** | ✅ 完整 Node.js | ✅ npm install | ✅ WASM 沙箱 | 高（CORS 头限制） | 每页单实例，对已有平台破坏性大 |

当前倾向 **自研 iframe runtime** 方案；Sandpack 仅用于对照验证“多文件装载 / 错误回传 / 启动时延”这类体验基线。

### 自研 Runtime MVP 边界

- **文件类型**：优先支持 `html` / `css` / `ts` / `tsx` / `js` / `jsx` / `json`
- **入口约定**：Canvas 必须声明单一入口文件（如 `src/main.tsx`）
- **依赖策略**：仅支持项目级白名单库，通过 import map 或固定 CDN URL 注入，不支持运行时任意安装 npm 包
- **刷新策略**：MVP 先走整页重载；只有在体验明显不足时再补文件级热刷新
- **错误呈现**：优先实现编译错误、运行时错误、缺失文件/缺失库三类错误卡片
- **静态资源**：图片 / 字体 / 二进制附件延后；首版以文本文件和 JSON 数据文件为主

### 更贴近现有代码的后端落点

1. **新增 Canvas 资产层**
   在 domain / application / api 层增加 Canvas 实体、repository、CRUD 与 runtime snapshot service。
2. **新增 `canvas_fs` provider**
   provider 直接读写 Canvas 文件存储；能力上至少支持 `read/write/list`，最好补齐 edit capabilities 以支持 `fs_apply_patch`。
3. **扩展 session address space 构建**
   参考 `build_task_session_runtime_inputs()` 对 lifecycle mount 的追加方式，在 Project / Story / Task 对应的上下文构建流程里把 Canvas mount 追加进去。
4. **扩展 runtime tool capability**
   在 `FlowCapabilities`、`RelayRuntimeToolProvider::build_tools()`、`session_plan.rs` 三处同步加入 Canvas 工具。
5. **新增 Canvas runtime snapshot API**
   负责把 Canvas 文件 + 外部绑定文件解析成前端可直接装载的文件快照。

### 更贴近现有代码的前端落点

1. **系统事件层**
   在 `AcpSystemEventGuard.ts` / `AcpSystemEventCard.tsx` 增加 `canvas_presented`。
2. **页面状态层**
   在 `SessionPage.tsx` 维护 `activeCanvasId` / `isCanvasPanelOpen` 等状态，并在 `handleSystemEvent` 中解析 update payload。
3. **Canvas Panel 组件层**
   新增页面级 Canvas Panel，首版只挂在独立 SessionPage。
4. **Runtime 装载层**
   Panel 打开后按 `canvas_id + session_id` 拉取 runtime snapshot，装载 iframe 并监听错误/ready 消息。

### 安全边界

- iframe `sandbox="allow-scripts"`，**不含** `allow-same-origin`
- 可选 fetch proxy 解决 null-origin CORS
- CSP 头注入 srcdoc 限制外部资源域名
- 沙箱只暴露受控桥接 API，不向 Canvas 代码透出宿主页面 DOM / store / session token

### 数据通道

- **父 → iframe**：`postMessage` 下发运行时快照、刷新指令、受控元信息
- **iframe → 父**：`postMessage` 报告 `ready`、编译错误、运行时错误、渲染成功
- 沙箱内优先通过“文件读取约定”消费数据；如需更友好的 AI 编程体验，可额外提供薄封装（如 `useCanvasData(alias)`），但其底层仍指向绑定文件而非裸对象

### 推荐实施计划

#### Phase 0：技术 spike（1 周内）

- 在现有前端页面中完成最小 `iframe sandbox` demo
- 验证 `esbuild-wasm` 编译 TS/TSX、React 挂载、ECharts 渲染
- 验证 import map / 固定 CDN 白名单库加载
- 验证 `postMessage` 错误回传、启动时延和整页重载体验
- 以同等 demo 简单对照 Sandpack 启动时延、嵌入复杂度和调试成本

#### Phase 1：后端模型与挂载（1~1.5 周）

- 落地 Canvas 实体 / repo / API / mount provider / 常量注册
- 提供 `create_canvas`、`present_canvas`、`inject_canvas_data` 等最小工具
- 打通 Canvas mount 加入 Session address space 的流程（参考 lifecycle mount 的追加模式）
- 明确数据绑定持久化格式和独立 runtime snapshot 接口
- 同步扩展 `FlowCapabilities`、`acp_sessions` request builder、`session_plan` 工具摘要

#### Phase 2：前端 Runtime 与 Panel（1~1.5 周）

- 实现 Canvas Panel 容器、加载态、错误态、刷新能力
- 实现 iframe bootstrap、import map 注入、运行时协议桥
- 接入 `canvas_presented` 系统事件，支持在 `SessionPage` 打开指定 Canvas
- 实现“读取快照 -> 启动运行 -> 渲染状态回传”的主链路

#### Phase 3：首批场景打磨（1 周）

- 交互式表格示例
- ECharts 统计图示例
- 文件绑定更新后的刷新体验和错误文案优化
- 形成给 Agent 使用的最小 Canvas 编码约定（入口、目录、数据别名）

#### Phase 4：视情况再做（P2）

- 文件级热刷新
- 更完整的样式/资源支持
- 交互事件回传 Agent
- Canvas 模板库与版本历史

### 与现有工具调用的关系示例

```
Agent 工作流：
1. shell_exec → 执行统计脚本 → 结果写入 lifecycle 产物
2. create_canvas(title="物件布设统计") → 创建 Canvas 实体 + mount
3. fs_write(canvas-xxx://src/main.tsx, content=...) → 写入可视化代码
4. inject_canvas_data(canvas_id, alias="stats", source="lifecycle://active/artifacts/{id}") → 绑定数据源
5. present_canvas(canvas_id) → 系统事件 → 前端打开 Canvas Panel
6. 前端请求运行时快照：Canvas 文件 + `/bindings/stats.json` → iframe 启动渲染
```
