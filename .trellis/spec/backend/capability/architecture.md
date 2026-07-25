# Capability Architecture

Capability分为产品期望与Runtime实际保证：AgentFrame/feature sources编译`AgentSurfaceSnapshot`；Integration service提供`RuntimeOffer`；admission求交并持久化`BoundAgentSurface`；adapter回报`AppliedAgentSurface`。

- capability profile是正交guarantee集合，不通过connector类型或level OR派生。
- required contribution未被精确应用时command/tool不可用。
- VFS、MCP、Hook、Skill与tool catalog作为Business Surface输入；Tool Broker在调用时再次校验binding/generation/capability/VFS，并通过独立 AgentRun permission facade 判定执行授权。
- external Agent不能注入工具时如实声明Unsupported/Observed/PromptOnly，不伪装Exact。
- UI只消费bound profile provenance与command availability。

测试覆盖surface deterministic compile、offer intersection、required拒绝、stale generation与UI availability。
