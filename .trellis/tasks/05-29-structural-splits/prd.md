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

## Scope（design 阶段细化，下面是方向）

1. **抽 `agentdash-application-ports` crate**（沿盲审 + 第一波已识别缝）：
   - 内含 `backend_transport` + SPI 风格 port（`RelayPromptTransport`/`BackendTransport`/`ExtensionRuntimeActionTransport`）+ 其 DTO（零 use-case 依赖）。
   - 加入 workspace members；`application` 与 `api` 改依赖此 crate。
   - 打断 `task` 枢纽：让 `task` 依赖 ports crate 而非兄弟 concretes；给 `vfs` 暴露 facade 阻止 companion 伸内部。
   - `runtime_gateway`/`runtime`/`runtime_bridge` 命名消歧 + 内部分组（不强制独立 crate）。
2. **session 目录重排**（纯文件移动 + mod 重连）：`runtime/`、`composition/`、`lifecycle/`、`hooks/`、`eventing/`；`memory_persistence.rs` 移出 `src/` 到 test-support。
3. **拆前端 god-component**：每个 >1000 行组件拆为 presentational + `model/` hook；`SettingsPageContent` 一节一文件。打断 `extension-runtime↔workspace-panel↔canvas-panel` 循环（共享契约移中性模块）。
4. god-file（`companion/tools.rs` 等）按 tool/provider 拆分，`#[cfg(test)]` 大块移 `tests/`，逆转 `hook_action.rs` 合并。

## 协调与风险

- **与甲类协调**：前端 god-component 优先尝试 reopen `frontend-server-state-refactor` 完成 stage C；若 reopen 困难才在本 child 做。
- 拆 crate 影响全工作区 import，**一次一 crate、每步 build-gate**，禁止多 crate 并行拆。
- 遵守第一波教训：动前先调查实际耦合，但"耦合被高估"不作默认开脱。

## Acceptance Criteria（硬指标 + 验收命令）

- [ ] `agentdash-application-ports` crate 存在且为 `Cargo.toml` workspace member（grep）；`backend_transport` 等 port 已迁入
- [ ] `task` 模块 `use crate::{...}` 对兄弟 concrete 模块（canvas/capability/runtime/vfs/workflow/...）的直依赖未新增、且能下沉者改依赖 ports（journal 列改动前后计数）
- [ ] `companion/tools.rs` 不再 `use crate::vfs::tools::provider` 内部（grep = 0，改走 facade）
- [ ] `session/` 顶层目录按职责分组（`runtime`/`composition`/`lifecycle`/`hooks`/`eventing` 子目录出现）；`memory_persistence.rs` 不在 `src/` 下（`find crates/agentdash-application/src -name memory_persistence.rs` 空）
- [ ] `runtime.rs`/`runtime_bridge.rs` 改名消歧（gateway 外无 `runtime` 前缀文件，journal 记录映射）
- [ ] 前端目标 god-component（`SettingsPageContent`/`activity-inspector`/`SessionChatView`/`workspace-list`）行数各 < 600 或拆为目录（`wc -l`）；`extension-runtime↔workspace-panel↔canvas-panel` 循环打断（madge/grep 无双向边）
- [ ] 每步 `cargo check --workspace` / `tsc --noEmit` exit 0

> **范围边界**：`application-session`/`application-workflow` 物理拆 crate **不在本轮 AC**（已决策推后）；不得因未拆这两者判本 child 未完成。
