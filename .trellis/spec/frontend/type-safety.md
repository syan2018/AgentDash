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

---

## 禁止模式

- `any` 类型
- `as SomeType` 类型断言（除非编译器无法推断的极少数场景）
- `value!` 非空断言
- API 响应直接信任为具体类型（必须经过 mapper）
