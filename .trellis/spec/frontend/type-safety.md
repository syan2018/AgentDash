# Type Safety

> 前端类型安全规范。

---

## 核心原则

- **严格模式**：TypeScript strict 已启用，禁止 `any`、类型断言（`as`）、非空断言（`!`）
- **snake_case 直接映射**：前端类型字段名与后端 Rust DTO 直接对齐，不做 camelCase 转换
- **运行时验证**：API 响应通过 mapper 函数做 `unknown → typed object` 转换，包含基础校验

---

## 类型分层

| 位置 | 用途 |
|------|------|
| `types/index.ts` + 拆分文件 | 跨 Feature 共享的领域类型 |
| `features/{name}/model/types.ts` | Feature 内部类型 |
| `generated/backbone-protocol.ts` | 自动生成的协议类型，禁止手动修改 |

---

## Mapper 边界

mapper 只负责：
- `unknown → typed object` 转换
- 状态值归一化（如多个旧状态名映射到新枚举值）
- null / array / number 基础运行时校验

mapper 不负责：
- 同时兼容 `camelCase` + `snake_case`（出现 `fooBar ?? foo_bar` 时应修后端 DTO）
- 猜测后端字段别名

---

## CapabilityDirective 契约

`CapabilityDirective` 使用 qualified path 字符串（`{ add: string } | { remove: string }`），支持能力级、工具级、MCP 能力。`CapabilityKey` 仅用于前端内置能力选项的 UI 展示，不要用它收窄 API 配置中的 `capability_directives`。

## Session Runtime Projection DTO

Session workspace panel、context overview 与 VFS tab 以 `runtime_surface: ResolvedVfsSurface` 作为运行时 mount 展示、默认 mount、可浏览性与编辑能力的唯一 UI 输入。`ExecutionVfs` 保留为 session context DTO 中的 runtime 诊断信息；界面读取 final projection DTO，可以保证 pending runtime patch、VFS overlay 与后端 capability projection 完成后，前端展示的是最终生效的地址空间。

Project / Story / Task / Agent knowledge 等预览入口使用 `ResolvedVfsSurfaceSource` 解析 preview surface；Session 入口直接消费 `session_runtime` 的 `runtime_surface`。两类入口共享 VFS browser 组件，但各自的 surface 来源显式表达，方便在跨层测试里验证 preview 与 runtime 语义。

---

## 禁止模式

- `any` 类型
- `as SomeType` 类型断言（除非编译器无法推断的极少数场景）
- `value!` 非空断言
- API 响应直接信任为具体类型（必须经过 mapper）
