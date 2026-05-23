# AppState Bootstrap 拆分

## Goal

将 `AppState::new_with_plugins` 的系统装配拆成清晰的 bootstrap/kernel 模块，降低 composition root 复杂度，同时保持 HTTP API、service 行为和外部初始化语义稳定。

## Confirmed Facts

- 架构 review 共同指出 `crates/agentdash-api/src/app_state.rs` 同时负责 repository、plugin、shared library seed、VFS、relay、session runtime、auth、routine、scheduler、background worker 等装配。
- `.trellis/spec/backend/architecture.md` 当前将 `agentdash-api` 定位为 HTTP 路由、DTO、中间件和 AppState 装配层。

## Requirements

- 拆分仅改变装配结构，不改变业务行为和路由契约。
- 新增 `agentdash-api/src/bootstrap/` 或等价模块组织，至少覆盖 repositories、plugins、vfs、relay、session、auth、routine/background workers。
- 每个 bootstrap/kernel 暴露明确输入、输出和依赖顺序。
- 对延迟注入点进行集中命名和说明，为后续 staged builder 或显式 init graph 做准备。
- 增加轻量验证，避免 bootstrap 模块反向依赖 routes helper 或让 routes 被下层模块引用。

## Acceptance Criteria

- [ ] `AppState::new_with_plugins` 缩减为高层构造顺序，不再直接包含所有服务细节。
- [ ] repository/plugin/vfs/relay/session/auth/routine/background worker 至少拆成独立装配函数或模块。
- [ ] 新模块中的循环/延迟注入点有集中说明。
- [ ] 现有启动、测试和路由行为不变。
- [ ] 增加或更新 spec，记录 AppState 与 bootstrap/kernel 的职责边界。

## Out of Scope

- 不重写 DI 框架。
- 不拆 crate 到 `agentdash-host`。
- 不顺手重构业务 service 逻辑。
