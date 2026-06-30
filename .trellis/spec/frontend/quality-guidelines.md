# 前端质量规范

> AgentDashboard 前端代码质量标准。

---

## 概览

- **Linting**: ESLint + typescript-eslint
- **构建**: TypeScript 严格模式（`~5.9.3`）
- **测试**: Vitest（`^4.0.18`）
- **局部检查命令**: `pnpm run frontend:check`、`pnpm run frontend:lint`、`pnpm run frontend:test`
- **仓库质量门**: `node scripts/quality-gates.js run <gate>`，其中 gate 组成由 `scripts/lib/quality-gates.js` 维护

代码提交前必须通过对应改动面的质量门、lint 和类型检查。

## 质量门事实源

仓库级质量门使用 `scripts/lib/quality-gates.js` 维护 gate 与 step 的组成关系，`scripts/quality-gates.js run <gate>` 是本地脚本和 CI workflow 共同调用的入口。workflow 只保留 checkout、依赖安装、缓存、浏览器安装和 artifact 上传等执行环境编排，具体检查命令从 manifest 展开。

这样做的原因是 PR quick、deployment contract、desktop check、full local 等检查集合需要在本地和 CI 中保持同一套选择规则；manifest 测试会校验 root script 与 workflow 继续委托给 runner，避免质量门集合随入口漂移。

CI workflow 的 `pnpm/action-setup` 不单独声明 pnpm 版本，版本由 `package.json` 的 `packageManager` 字段提供。这样本地默认包管理器版本和 CI 安装版本共享同一事实源。

---

## 禁止模式

| 禁止模式 | 原因 | 替代方案 |
|----------|------|----------|
| `any` 类型 | 失去类型安全 | 使用 `unknown` 并做类型守卫 |
| `!` 非空断言 | 运行时可能出错 | 先做 null 检查 |
| 内联样式 `style={{}}` | 难以维护 | 使用 Tailwind 类名 |
| 直接修改 props | 违反 React 原则 | 使用回调或状态提升 |
| `console.log` 调试代码 | 泄漏到生产 | 提交前清理 |
| `fooBar ?? foo_bar` 双读字段 | 掩盖后端契约错误 | 修正后端 DTO，前端只读规范字段 |

---

## 必备模式

- 组件 props 使用 TypeScript interface 定义
- Hook 返回类型显式声明
- **API 响应字段直接使用 `snake_case`，不做 camelCase 转换**
- Store mapper 只负责 `unknown → typed object` + 状态值归一化
- 错误边界处理网络请求错误

> **关键决策**：前端类型直接使用 `snake_case` 与后端 Rust 实体对齐。
> 参见 [Type Safety](./type-safety.md) 中的详细说明和示例。

---

## 测试要求

- 关键功能使用 Vitest 编写单元测试
- Feature model 层（hooks、reducer、mapper）优先覆盖
- 测试命令：`pnpm --filter app-web test`

关键测试场景：
1. 基础功能（输入、点击、提交）
2. 错误场景（网络断开、服务端错误）
3. 边界情况（空输入、超长内容）
4. Stream 合并（事件归并、重复检测）

---

## Code Review 检查清单

- [ ] 类型定义完整，无 `any`
- [ ] 错误处理完善
- [ ] Props 有文档注释
- [ ] 无 `console.log` 调试代码
- [ ] 组件拆分合理
- [ ] API 字段使用 `snake_case`，无双风格字段解析
