# Cross-Layer Thinking Guide

> 跨层功能实现前的思考清单。大多数 bug 发生在层边界，而非层内部。

---

## AgentDash 关键边界

| 边界 | 常见问题 |
|------|---------|
| Agent ↔ ExecutionManager | Agent 输出格式不统一、执行超时 |
| ExecutionManager ↔ StateManager | 状态回写时机、失败状态处理 |
| Orchestration ↔ State | 编排策略不能直接修改状态（必须通过 StateManager） |
| Injection ↔ Task | 注入内容过大、注入时机 |
| Cloud ↔ Local VFS | mount 语义不一致、绝对路径泄漏、context 与 runtime tool 分叉 |
| Backend ↔ Frontend | 实时状态推送协议、断线重连 |

---

## 跨层错误模式

1. **编排层绕过状态层**：编排策略直接修改 Task/Story 字段，绕过 StateManager，导致状态历史缺失
2. **前端自行推断状态**：根据 artifacts 数量等间接信号推断 Task 状态，而非以后端 `status` 字段为准
3. **策略泄漏到接口**：接口暴露了实现细节（如 `createWorktree`），而非表达意图（如 `createWorkspace`）
4. **视图操作影响执行**：删除视图分组时意外修改了 Story 状态——视图关系是展示层概念
5. **产品binding存在 ≠ Driver可用**：必须同时验证Host binding generation/lease与canonical Runtime状态；断连收敛Lost后不能由旧generation复活

---

## 实现检查清单

**实现前：**

- [ ] 映射完整的数据流路径
- [ ] 确认不会绕过 StateManager 进行状态修改
- [ ] 确认视图操作不会影响核心状态
- [ ] 若涉及云端/本机文件访问，先定义 mount/provider/capability 边界（参考 `vfs-access.md`）
- [ ] 若涉及 runtime hook/workflow，确认"信息获取在 loop 外、控制决策在 loop 边界同步"（参考 `execution-hook-runtime.md`）

**实现后：**

- [ ] 验证 StateChange 历史完整记录
- [ ] 验证前端状态与后端状态一致
- [ ] 若引入新 runtime policy/metadata，验证前端看到的是真实生效的 runtime surface
