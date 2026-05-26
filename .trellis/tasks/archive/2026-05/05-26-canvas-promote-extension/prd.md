# Canvas Promote to Extension

> 父任务：`05-26-ts-extension-host-sdk`
> 状态：planning

## Goal

让现有 Canvas 能被打包成 extension template，安装到 Project 后作为 WorkspacePanel 插件 tab 运行。

## Requirements

- Canvas files、entry_file、sandbox_config、bindings 可映射到 extension package / manifest。
- Manifest 写入 workspace tab，renderer 使用 `canvas_panel` 或兼容的 webview/canvas renderer。
- 安装后 session projection 暴露 Canvas-derived tab contribution。
- 运行时复用 Canvas runtime preview 或其抽象能力。
- Source-status / update semantics 与其它 extension asset 一致。

## Acceptance Criteria

- [ ] Canvas 可发布为 extension template / package artifact。
- [ ] 安装到 Project 后 WorkspacePanel 出现对应 tab。
- [ ] Tab 能运行 Canvas entry。
- [ ] Packaged artifact 与 source-status 正常工作。
- [ ] 测试覆盖 Canvas -> ExtensionTemplate mapper。

## Out of Scope

- 不作为基础插件 MVP 的前置；等 local-hello 金线跑通后实现。
