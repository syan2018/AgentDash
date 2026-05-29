# 结构性拆分（application 89k 拆 crate / session 目录重排 / 前端 god-component）

> parent: `05-29-slop-cleanup-wave2`。盲审病灶 6（F）+ 甲类前端残留。类：丙+甲。Wave 4，**设计先行，禁止盲并行**。

## Goal

给 89k 的 `application` 巨石和 28k 的 `session/` 平铺目录引入真实编译/物理缝，拆掉前端剩余 god-component，打断前端跨 feature 循环。这是 L 风险，**必须先出 design.md 再动手**。

## 现状证据

- `application` 89,140 行、30+ 顶层模块、`lib.rs:1-32` 平铺 alphabetical `pub mod` 无分组无分层；`task/` 直 import `crate::{canvas,capability,platform_config,repository_set,runtime,runtime_bridge,session,vfs,workflow,workspace}`——伸进 10 兄弟模块的耦合枢纽；`companion/tools.rs:28` 伸进 `vfs::tools::provider` 内部。
- 命名簇 `runtime.rs`(MCP DTO)/`runtime_bridge.rs`(转换)/`runtime_gateway/`(provider 注册)/`extension_runtime.rs`(安装投影) 共前缀无共概念。
- god-file：`companion/tools.rs`(2795，自承吸并了 `hook_action.rs`)、`runtime_gateway/extension_actions.rs`(1551)、`canvas/tools.rs`(1327)、`capability/resolver.rs`(1141)。
- `session/` 28k 平铺 ~70 文件三薄子目录；`memory_persistence.rs`(1491) 是 `#[cfg(test)]` 测试夹具却躺 `src/`。
- 前端（甲类残留，`frontend-server-state-refactor` stage C 未完）：`SettingsPageContent.tsx`(2014，内联 ~15 组件)、`activity-inspector.tsx`(1304)、`SessionChatView.tsx`(1008)、`workspace-list.tsx`(1301)；`extension-runtime↔workspace-panel↔canvas-panel` 双向循环（~30 跨 feature 边）。

## 已拍板决策（2026-05-29）

- **本轮折中**：物理拆 crate **只做 `agentdash-application-ports`**（最独立、零 use-case 依赖，建第一道真缝）；其余按职责**内部模块重排**。
- **显式推后（非本轮欠账）**：`application-session` / `application-workflow` 物理拆 crate 推到后续轮次，本 child **不做也不算未完成**——AC 不含这两项。
- **DDD 依赖方向**：`domain` 不引用 contracts / protocol DTO；protocol/API/contracts 可以依赖 domain 或 application port 并向外映射。当前 `contracts::workflow` re-export domain 类型是 `domain-purification` 的后续债务，不在本 child 通过让 domain 依赖 contract 解决。
- **结构性拆分 scope 收敛**：本 child 以 ports crate、test support 剥离、前端循环打断与两个前端 god component 入口拆分为验收核心；session 全目录分桶和 `runtime.rs` / `runtime_bridge.rs` 命名消歧因 blast radius 大且刚完成 session assembly 收敛，记录为后续独立 slice，不作为本 child 完成阻塞。

## Scope（design 阶段细化，下面是方向）

1. **抽 `agentdash-application-ports` crate**（沿盲审 + 第一波已识别缝）：
   - 内含 `backend_transport` + SPI 风格 port（`RelayPromptTransport`/`BackendTransport`/`ExtensionRuntimeActionTransport`）+ 其 DTO（零 use-case 依赖）。
   - 加入 workspace members；`application` 与 `api` 改依赖此 crate。
   - 打断 `task` 枢纽：让 `task` 依赖 ports crate 而非兄弟 concretes；给 `vfs` 暴露 facade 阻止 companion 伸内部。
   - `runtime_gateway`/`runtime`/`runtime_bridge` 命名消歧 + 内部分组（不强制独立 crate）。
2. **session test support**：`memory_persistence.rs` 移出 `src/` 到 test-support，保留 `#[cfg(test)]` module surface。
3. **拆前端 god-component**：`SessionChatView.tsx` 降到 600 行以下；`workspace-list.tsx` 拆为目录入口。打断 `extension-runtime↔workspace-panel↔canvas-panel` 双向循环（共享 workspace runtime 契约移中性模块）。
4. god-file（`companion/tools.rs` 等）本轮只处理已明确的 `vfs::tools::provider` 内部引用，深拆工具文件留后续 slice。

## 协调与风险

- **与甲类协调**：前端 god-component 优先尝试 reopen `frontend-server-state-refactor` 完成 stage C；若 reopen 困难才在本 child 做。
- 拆 crate 影响全工作区 import，**一次一 crate、每步 build-gate**，禁止多 crate 并行拆。
- 遵守第一波教训：动前先调查实际耦合，但"耦合被高估"不作默认开脱。

## Acceptance Criteria（硬指标 + 验收命令）

- [x] `agentdash-application-ports` crate 存在且为 `Cargo.toml` workspace member；`backend_transport`、extension runtime action/channel transport、`VfsMaterializationTransport` 已迁入。
- [x] `task` 模块兄弟 concrete 依赖未新增；本批没有把 use case 编排伪装成 port。`companion/tools.rs`、`canvas/tools.rs`、`workflow/tools/advance_node.rs` 不再引用 `crate::vfs::tools::provider` 内部。
- [x] `memory_persistence.rs` 不在 `crates/agentdash-application/src` 下，改由 `test-support/session_memory_persistence.rs` 提供 test-only module。
- [x] 前端目标：`SettingsPageContent.tsx` 255 行、`activity-inspector.tsx` 336 行（上一 child 交叉完成）；`SessionChatView.tsx` 584 行；`workspace-list.tsx` 为 4 行目录入口。
- [x] `extension-runtime↔workspace-panel` 与 `extension-runtime↔canvas-panel` 双向循环打断；保留 workspace-panel 作为 tab composition root 对 extension/canvas 的单向组合依赖。
- [x] 验证通过：`cargo check --workspace`、`cargo test -p agentdash-application --lib`、`pnpm -C packages/app-web exec tsc --noEmit`。

> **范围边界**：`application-session`/`application-workflow` 物理拆 crate **不在本轮 AC**（已决策推后）；不得因未拆这两者判本 child 未完成。
