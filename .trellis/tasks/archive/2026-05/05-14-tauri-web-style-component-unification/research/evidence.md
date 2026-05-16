# 证据记录：Tauri 与 Web 样式差异

## 构建产物

本轮已运行：

```bash
pnpm --filter app-tauri build
pnpm --filter app-web build
```

观察到的主 CSS 产物：

| 入口 | 主 CSS | 大小 |
| --- | --- | --- |
| `app-web` | `packages/app-web/dist/assets/index-DHKZGoeN.css` | 109,712 bytes |
| `app-tauri` | `packages/app-tauri/dist/assets/index-BDR5Taqb.css` | 38,260 bytes |

这说明桌面端构建没有生成完整 Web Dashboard 所需 utility。

## 样式入口差异

`packages/app-web/src/main.tsx`：

```ts
import './styles/index.css'
import 'streamdown/styles.css'
import 'katex/dist/katex.min.css'
```

`packages/app-tauri/src/main.tsx`：

```ts
import 'app-web/styles.css'
import './styles.css'
import '@agentdash/views/local-runtime.css'
```

两端的全局样式集合不一致。

## 变量污染

`packages/app-web/src/styles/index.css` 使用 HSL token：

```css
--primary: 217 92% 50%;
--muted: 220 12% 88%;
--border: 220 14% 91%;
```

并生成：

```css
color: hsl(var(--primary));
border-color: hsl(var(--border));
```

`packages/views/src/local-runtime/LocalRuntimeView.css` 当前在 `:root` 定义：

```css
--border: #d8dee8;
--muted: #647084;
--primary: #2563eb;
```

这会把 Web 的 HSL token 覆盖为 HEX 值，导致 `hsl(var(--primary))` 这类规则失效。

## 参考任务结论

来自 `.trellis/tasks/05-13-multica-local-runtime-concept-alignment/research/desktop-local-integration.md` 的相关结论：

- 学习 multica 的 `core/views/ui/app` 分层。
- desktop 应是本机能力控制台，不只是 Web wrapper。
- IPC bridge / Tauri command 应隔离本机能力，前端业务视图只依赖类型化端口。
- desktop-only settings slot 可以避免 fork 整套 Web 设置页。
- 不要一次性重构全部前端到 packages，优先从 desktop 需要复用的 query/services/views 开始。

这些结论支持本任务采取“先统一样式契约，再渐进抽取 views/core”的路线。

## 工程化方案补充

当前 workspace 配置为：

```yaml
packages:
  - 'packages/*'
```

已有 package：

- `app-web`
- `app-tauri`
- `core`
- `views`

因此新增 `packages/ui` / `@agentdash/ui` 不需要改变 workspace 结构，符合现有 monorepo 组织方式。

标准化后的目标依赖方向：

```text
@agentdash/core  -> 类型、端口、纯函数
@agentdash/ui    -> 设计系统、Tailwind 入口、基础组件
@agentdash/views -> 业务视图，依赖 core + ui
app-web          -> Web 宿主，依赖 views/core/ui
app-tauri        -> Desktop 宿主，依赖 views/core/ui + Tauri API adapter
```

这比让 `app-tauri` 直接依赖 `app-web/styles.css` 更标准：视觉事实源不再属于某个具体宿主。

## 最终决策

用户明确不希望为了已有技术债做兼容式折中。因此最终规划采用完整终态：

- 新增 `@agentdash/ui`，作为唯一设计系统事实源。
- `app-web` 和 `app-tauri` 直接导入 `@agentdash/ui/styles.css`。
- 移除 `app-web/styles.css` 跨包样式入口。
- 移除 `@agentdash/views/local-runtime.css` export。
- 移除 `app-tauri/src/styles.css` 全局桌面壳样式入口。
- Local Runtime 和 Desktop shell 改用 Tailwind class / `@agentdash/ui` primitives。
