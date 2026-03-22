# 类型系统拆分与组件结构重构 (P2) — 仅追踪

## Goal
拆分 types/index.ts 巨型文件，清理未消费类型，拆分 Panel 组件的数据/展示职责。

## Requirements
1. 按领域拆分 types/index.ts 为 workflow.ts, hook.ts, session.ts 等
2. 清理 UI 层从未消费的类型（TaskSessionRuntimePolicySummary 等）
3. 将 ProjectWorkflowPanel / TaskWorkflowPanel 拆分为 hook + 纯展示组件
4. 清理 HookEventData 局部定义，统一到共享类型
5. 消除 SessionChatView 中的 "__placeholder__" magic string

## Status
仅追踪，待后续排期实施。
