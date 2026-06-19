# Work Item 03: Projection Metadata Refresh

## Status

Planned.

## Goal

让 provider 必需 metadata 成为 lifecycle mount 的唯一运行时 contract，并把 SkillAsset/message stream/node facts 的刷新放回 projector。

## Scope

- 删除或降级 `agent_run_lifecycle_surface` 嵌套 metadata。
- 保留 provider 必需 metadata：`scope`、run/session/node fields、`writable_port_keys`、`skill_asset_project_id`、`skill_asset_keys`。
- Projector 一次性生成 SkillAsset projection metadata，不再依赖旧 mount metadata 回读事实。
- 明确 VFS overlay / mount directive 的整 mount replace 与 lifecycle projection refresh 的局部事实重算边界。

## Affected Areas

- `crates/agentdash-application/src/lifecycle/surface/surface_projector.rs`
- `crates/agentdash-application/src/vfs/mount_skill_asset.rs`
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
- `.trellis/spec/backend/vfs/architecture.md`

## Dependencies

依赖 Work Item 01 的 typed facts；可与 Work Item 02 部分并行，但最终需要 builder 边界一致。

## Validation

- `cargo test -p agentdash-application lifecycle_skill_projection`
- `cargo test -p agentdash-application lifecycle::surface::surface_projector`
- `cargo test -p agentdash-application vfs::provider_lifecycle`
