# Complete Agent 持久化边界回顾

## 1. Root Cause Category

- **Category**: B / E — Cross-Layer Contract 与 Implicit Assumption
- **Specific Cause**: 旧模型把当前进程可调用的 service registration 当成跨重启事实，并让逻辑
  instance、Host binding generation、connection epoch 与 remote generation互相代替。数据库因而
  同时承担在线目录和执行恢复两种生命周期不同的权威。

## 2. Why Earlier Fixes Could Not Close the Loop

1. 仅隔离可选 Codex 启动失败只能保护应用启动，无法解释旧 binding 应该解析哪个新进程 handle。
2. 仅允许 placement 覆盖会让旧 generation 命中新 attachment，破坏 restart fencing。
3. 仅把 attachment 加入 Host target 仍不足以恢复 Native/Remote：Product 必须保留完整 execution
   profile，Remote 必须显式区分 Host generation 与 transport generation。
4. 单包测试能证明类型和局部行为，却发现不了 replacement 未 retire、lease normalized row 残留、
   descriptor 二次读取和 callback generation 反向映射等跨层 authority 漂移。

## 3. Prevention Mechanisms

| Priority | Mechanism | Specific Action | Status |
| --- | --- | --- | --- |
| P0 | Architecture | process-local Live Catalog 与 durable Host repository 分离 | DONE |
| P0 | Compile-time | binding/runtime target/lifecycle effect统一携带 exact target snapshot | DONE |
| P0 | Persistence | `0089` 删除五张 live inventory 表并保存完整 Product execution profile | DONE |
| P0 | Runtime fence | attachment/incarnation、Host generation、remote generation与callback route显式映射 | DONE |
| P0 | Test coverage | 跨 incarnation、replacement retire、Product restart、Remote generation、lease release与双次进程启动 | DONE |
| P1 | Review guide | 跨层检查清单要求逐一标出四类 identity/generation owner | DONE |

## 4. Systematic Expansion

- **Similar Issues**: 后续数据库清理应继续用“跨重启是否不可重建、是否需要幂等恢复、是否有唯一
  owner”判断，而不是按当前表是否被代码读取判断。
- **Design Improvement**: 每个 durable snapshot 都应对应一个恢复决策输入；每个 live registry
  entry 都应覆盖明确的 process/connection epoch，并且不存在逻辑键 fallback。
- **Process Improvement**: 涉及 lifecycle identity 的改动必须同时运行空库 migration、已有开发库
  顺序升级、normalized projection 删除语义和同一数据根双次启动。

## 5. Knowledge Capture

- [x] `agent-runtime-driver-host.md` 固定 live attachment、Remote generation 与 callback route契约。
- [x] `agent-runtime-persistence.md` 固定 durable/normalized projection精确一致性。
- [x] `agent-runtime-agentrun-facade.md` 固定 Product execution profile驱动的重启恢复。
- [x] `cross-layer-thinking-guide.md` 增加多生命周期 identity/generation owner检查。
- [x] 当前任务包含对应单元、embedded PostgreSQL与进程级回归证据。
