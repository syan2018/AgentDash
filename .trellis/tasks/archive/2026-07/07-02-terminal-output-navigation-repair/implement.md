# 终端输出展示与跳转修复实施计划

## Checklist

1. 建立 terminal projection helper
   - 提取 live/history 共用的 terminal platform event projector。
   - 增加 history hydrate 对 terminal event 的幂等投影。
   - 保持 reducer 纯净，terminal event 继续不进入聊天 entries。

2. 扩展 terminal store capability
   - 为 terminal info 增加 interactive/read-only/state-only 能力字段。
   - `terminal_state_changed` 可创建最小 terminal projection。
   - 补充 store tests。

3. 修 terminal/output tab 行为
   - 真实 terminal 才发送 input/resize。
   - 命令输出 replay 使用只读 tab 或输出查看 tab。
   - active id 切换和 output base offset 保持无重复回放。

4. 修 workspace 跳转
   - 从页面层向 session/card 注入 open workspace panel action。
   - 命令卡片点击走 open-and-expand，不直接写全局 tab store。
   - 无可用 panel action 时禁用或保留卡内查看，不制造隐式 tab。

5. 修后端 terminal command response
   - input/resize/kill 匹配 relay response 类型。
   - relay/local error 映射为 API error。
   - 增加 API handler tests。

6. PowerShell 验收
   - 增加 Windows-gated local/runtime 或 integration test，覆盖对象输出命令。
   - 手动验收时在 terminal tab 输入 `pwd`、`Get-Location`、`dir`、`Get-ChildItem`、`Write-Output (Get-Location).Path`。

7. Environment ContextFrame Windows 提示
   - 在 `environment_context_frame` 中按 Windows platform 追加 PowerShell 文本输出提示。
   - 非 Windows platform 不出现该提示。
   - 单元测试覆盖 rendered text 与 Environment section summary/body 的可见性。

8. 入口约束检查
   - 搜索确认未新增旧 Session 形态终端入口。
   - 新前端代码不直接拼装旧 Session 形态 terminal path。

## Validation Commands

- `pnpm --filter @agentdash/app-web test -- terminal`
- `pnpm --filter @agentdash/app-web test -- CommandExecutionCardBody`
- `cargo test -p agentdash-api terminal`
- `cargo test -p agentdash-local shell_session_manager`
- `cargo test -p agentdash-application-runtime-session environment_context_frame`
- Windows 环境执行 PowerShell terminal 手动验收。

## Risk Points

- history projector 幂等性不足会导致 output 重复。
- terminal state 先于 output 或注册到达时，store 合并逻辑需要保留已有 buffer。
- command output replay 如果复用 terminal tab，需要彻底禁用 input/resize 副作用。
- API route 迁移可能与并行旧 Session API 清理分支冲突；本任务只处理终端闭环必须触及的入口。

## Sub-Agent Dispatch

Phase 2 可拆为两条实现线：

- `trellis-implement` 终端前端线：projection、store capability、terminal/output tab、workspace navigation。
- `trellis-implement` 后端线：terminal command response handling、PowerShell Windows-gated tests。

Phase 2 check 需要覆盖前后端合同、入口搜索和 PowerShell 验收记录。
