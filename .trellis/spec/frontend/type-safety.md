# Type Safety

> 前端类型安全规范。

---

## 核心原则

- **严格模式**：TypeScript strict 已启用，禁止 `any`、类型断言（`as`）、非空断言（`!`）
- **snake_case 直接映射**：前端类型字段名与后端 Rust DTO 直接对齐，不做 camelCase 转换
- **Generated wire 单源**：内部 API 响应通过 `src/generated/*` 的 contract type 消费，service 层信任 generated wire，不做逐字段 identity rebuild

---

## 类型分层

| 位置 | 用途 |
|------|------|
| `types/index.ts` + 拆分文件 | 跨 Feature 共享的领域类型 |
| `features/{name}/model/types.ts` | Feature 内部类型 |
| `generated/backbone-protocol.ts` | 自动生成的协议类型，禁止手动修改 |
| `generated/*-contracts.ts` | Rust contract crate 生成的 HTTP / NDJSON DTO，作为跨层 wire type 来源 |

---

## Mapper 边界

mapper 只负责：
- UI view model 转换
- 外部/用户输入、第三方 payload、iframe/plugin bridge 等非内部 API 边界的 `unknown → typed object` 转换
- 尚未进入 contract crate 的 route-local 过渡 DTO

mapper 不负责：
- 同时兼容 `camelCase` + `snake_case`（出现 `fooBar ?? foo_bar` 时应修后端 DTO）
- 猜测后端字段别名
- 重新声明后端 enum/string union；跨层 DTO 的联合类型来自 `src/generated/*`
- 对 generated DTO 做逐字段 identity rebuild

## Generated Contract Boundary

前端把 `src/generated/*` 当作 wire DTO 事实源。Feature 可以定义 view model，但 view model 必须由 generated DTO 显式转换而来，原因是 UI 形态与 transport 形态有不同变化节奏。

Project extension runtime surface 消费 `generated/extension-runtime-contracts.ts`，`services/extensionRuntime.ts` 只保留 endpoint 调用与 webview asset URL 拼装。`features/extension-runtime` 以 Project ID 为 key 缓存 runtime projection，并向 WorkspacePanel 输出 tab descriptor 与 webview bridge；installation 的 `installed_source` 与 `package_artifact` 是显式可空字段，用来区分 Shared Library 安装来源与 packaged artifact 安装来源；前端不从 Shared Library payload 或 Session Context 推断 extension runtime 声明。

新增或修改跨层 DTO 时同步运行：

```powershell
pnpm run contracts:check
```

---

## CapabilityDirective 契约

`CapabilityDirective` 使用 qualified path 字符串（`{ add: string } | { remove: string }`），支持能力级、工具级、MCP 能力。`CapabilityKey` 仅用于前端内置能力选项的 UI 展示，不要用它收窄 API 配置中的 `capability_directives`。

## Session Runtime Projection DTO

Session workspace panel、context overview 与 VFS tab 以 `runtime_surface: ResolvedVfsSurface` 作为运行时 mount 展示、默认 mount、可浏览性与编辑能力的唯一 UI 输入。`ExecutionVfs` 保留为 session context DTO 中的 runtime 诊断信息；界面读取 final projection DTO，可以保证 pending runtime patch、VFS overlay 与后端 capability projection 完成后，前端展示的是最终生效的地址空间。

Project / Story / Task / Agent knowledge 等预览入口使用 `ResolvedVfsSurfaceSource` 解析 preview surface；Session 入口直接消费 `session_runtime` 的 `runtime_surface`。两类入口共享 VFS browser 组件，但各自的 surface 来源显式表达，方便在跨层测试里验证 preview 与 runtime 语义。

Session 右侧 WorkspacePanel 消费 current runtime projection state。该 state 以 `session_id + owner/source key` 为边界，携带 loading / ready / refreshing / error 状态；key 不匹配时不暴露上一份 runtime surface、capabilities 或 context snapshot。`workspace_module_presented`、`capability_state_changed` 等事件只触发当前 state 的 invalidate/refetch，界面不创建新的长期快照事实源。Canvas 打开动作读取 generated event payload 的 `presentation_uri`，值为 `canvas://{mount_id}`；前端不从 `view_key`、`module_id` 或 `cvs-<mount_id>://...` 推断 tab URI。

---

## 禁止模式

- `any` 类型
- `as SomeType` 类型断言（除非编译器无法推断的极少数场景）
- `value!` 非空断言
- 为 generated DTO 编写逐字段 identity mapper
