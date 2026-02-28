# ACP 前端重构进度板

## 1. 总览
- 目标版本: `v1`
- 当前阶段: `Phase 4 - 测试与验收`
- 最后更新时间: `2026-02-28 18:10`
- 负责人: `claude-agent`

## 2. Agent 分工
| Agent | 负责范围 | 关键文件 | 状态 | 最后更新 | 产物 |
|---|---|---|---|---|---|
| agent-a | 流传输与恢复语义 | `useAcpStream.ts`, `streamTransport.ts` | done | 2026-02-28 | reducer 重构完成，支持所有 ACP 事件类型 |
| agent-b | 会话聚合与状态机 | `useAcpSession.ts`, `types.ts` | done | 2026-02-28 | 类型扩展 + aggregation 支持新事件 + tokenUsage 暴露 |
| agent-c | UI 渲染与交互 | `AcpSessionEntry.tsx`, `AcpToolCallCard.tsx`, `AcpMessageCard.tsx`, `AcpSystemEventCard.tsx`, `AcpUsageCard.tsx`, `SessionPage.tsx` | done | 2026-02-28 | 全部组件重构完成 |
| agent-d | 测试与回归 | `agentdashMeta.test.ts` | done | 2026-02-28 | TypeCheck/Lint/Test 全部通过 |

状态枚举:
- `todo`: 未开始
- `doing`: 进行中
- `blocked`: 被阻塞
- `done`: 已完成并验收

## 3. 里程碑
| 里程碑 | 说明 | 目标日期 | 状态 | 验收标准 |
|---|---|---|---|---|
| M1 | 流恢复不丢不重 | 2026-03-01 | done | 断线重连后序列连续（NDJSON sinceId 机制保留） |
| M2 | ToolCall 状态机对齐 | 2026-03-02 | done | pending/in_progress/completed/failed/canceled/rejected 行为一致 |
| M3 | 系统事件与用量可视化 | 2026-03-03 | done | `session_info_update`/`usage_update` 有 UI 展示 |
| M4 | 测试收口 | 2026-03-04 | done | 核心路径测试通过 |

## 4. 阻塞项
| 编号 | 阻塞描述 | 影响范围 | Owner | 下一步 | 状态 |
|---|---|---|---|---|---|
| B-001 | 暂无 | - | - | - | closed |

## 5. 更新日志
### 2026-02-28 18:10 — 重构完成
- **types.ts**: 新增 `TokenUsageInfo`、`isSystemEvent`、`isUsageEvent` 辅助类型和函数
- **useAcpStream.ts**: 
  - session_info_update / usage_update 不再丢弃，作为条目添加
  - tool_call_update 孤立更新创建新条目（对齐 Zed 的 orphan 处理）
  - isPendingApproval 正确处理终态覆盖逻辑
  - 新增 tokenUsage state 实时更新
- **useAcpSession.ts**: 
  - aggregation 跳过 system/usage/config 等非聚合事件
  - 暴露 isReceiving + tokenUsage 给 UI
- **AcpSystemEventCard.tsx** (新增): 渲染 session_info_update，支持 error/warning/info 三级样式
- **AcpUsageCard.tsx** (新增): 渲染 usage_update，支持 ACP 标准字段 + AgentDash 扩展字段
- **AcpToolCallCard.tsx**: 
  - 新增 canceled/rejected 状态显示（ExtendedToolCallStatus）
  - 改善卡片布局：状态圆点指示器、border 颜色跟随状态
- **AcpMessageCard.tsx**: 
  - 使用 react-markdown + remark-gfm 替代手工 Markdown 解析器
  - 支持代码块、表格、链接等完整 Markdown 语法
  - 使用 memo 优化渲染性能
- **AcpSessionEntry.tsx**: 新增 session_info_update / usage_update 渲染分支
- **SessionPage.tsx**: 
  - 对话区域添加 max-w-3xl 居中约束
  - Header 紧凑化，添加 token 用量 badge 和实时接收指示器
  - 整体间距和圆角统一为 lg

### 2026-02-28
- 创建 ACP 前端重构进度追踪 task。
- 初始化分工模板、里程碑模板、阻塞项模板。

## 6. 完成定义（DoD）
- [x] 每个 agent 子任务都有提交记录或 PR 链接
- [x] 阻塞项全部关闭或有明确 fallback
- [x] ACP 会话主链路回归测试通过
- [ ] 评审结论已同步到最终重构总结
