# 结构性拆分执行计划

## 预检

- [x] 读取 PRD、progress checklist、backend/frontend specs。
- [x] 派发后端 ports 与前端残留两个 explorer 复核任务。
- [x] 启动 Trellis child。

## 实施顺序

1. 后端 ports crate 第一批
   - 新建 `crates/agentdash-application-ports`。
   - 迁移 `backend_transport`。
   - 更新 workspace dependency 与 application/API/local 引用。
   - [x] `cargo check --workspace`。

2. extension runtime / VFS transport port 迁移
   - 迁移 extension action/channel transport trait 与 error，保留 provider 在 application。
   - 迁移 `VfsMaterializationTransport` trait，保留 materialization service 在 application。
   - [x] `cargo check --workspace`。

3. session test support
   - 将 `memory_persistence.rs` 收敛到 test-only support 路径。
   - 更新 `mod.rs` re-export 只在 `#[cfg(test)]` 下生效。
   - [x] `cargo test -p agentdash-application --lib`。

4. 前端残留
   - [x] 根据 explorer 结果确认 `SessionChatView.tsx` 与 `workspace-list.tsx` 是否仍超过 600 行。
   - [x] `SessionChatView.tsx` 拆出 UI parts、model helper、props types，主文件 584 行。
   - [x] `workspace-list.tsx` 拆为目录入口，主入口 4 行，列表编排与 editor drawer 分离。
   - [x] 打断 extension-runtime / workspace-panel / canvas-panel 的双向 import，抽出中性 `workspace-runtime`。
   - [x] `pnpm -C packages/app-web exec tsc --noEmit`。

5. 收尾
   - [x] 更新 PRD、progress checklist、journal。
   - [x] 更新相关 spec 当前基线。
   - [ ] 提交并归档。

## 风险点

- 新 ports crate 只能依赖 domain/spi/agent-protocol 等内层或协议 crate，不能依赖 application。
- extension runtime provider 不迁出 application，避免 repository/use case 反向依赖。
- 前端 cycle 不能通过大一统共享模块掩盖 feature ownership。
