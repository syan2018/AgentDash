---
name: 终端执行 local/server 传递与前端展示
overview: |
  实现 session 中 shell 工具执行结果在 local backend 和 server 之间的完整传递通道，
  以及前端 Terminal Tab 中交互式终端输出流的实时展示。
  本任务是 05-05-browser-tab-workspace-panel 中 Terminal Tab 占位的后续完整实现。
todos:
  - id: design
    content: "设计 Terminal 执行协议 — 定义 local↔server 之间的 shell 执行事件格式"
    status: pending
  - id: backend-relay
    content: "实现后端 Terminal 执行结果中继 — local 执行结果 → server → 前端推送"
    status: pending
  - id: frontend-terminal
    content: "前端 Terminal Tab 完整实现 — xterm.js 或等效方案展示实时输出流"
    status: pending
  - id: integration
    content: "集成到 Tab 系统 — Terminal TabType 从占位符升级为完整功能"
    status: pending
isProject: false
---

# 终端执行 local/server 传递与前端展示

## 背景

当前 session 中 Agent 可以通过 shell 工具在 local backend 上执行命令，
但执行结果的展示仅限于聊天消息流中的工具调用卡片。
本任务的目标是实现完整的终端执行通道，使前端能在右栏 Terminal Tab 中
实时流式展示 shell 执行的输出，类似 IDE 内置终端的体验。

## 依赖

- 依赖 `05-05-browser-tab-workspace-panel` 提供的 Tab 系统基础设施
- 依赖 local backend 的 shell 执行能力

## 后续细化

此任务的详细设计待 Tab 系统基础设施完成后进一步展开。
