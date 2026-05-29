# implement — quickfix-swarm

## 执行模型

orchestrator（主控）按下表一次性派发 9 路 subagent（`trellis-implement`，opus，失败回退 sonnet）。每路 dispatch 提示首行须为 `Active task: .trellis/tasks/05-29-quickfix-swarm`，随后只给该路的 S 编号 scope + 文件清单 + 验证命令，并强调：**只动本路 crate/包，不 commit，不碰非目标项**。

## 派发批次

**批次 A（7 路完全不相交，立即齐发）**：S1 S2 S3 S6 S7 S8 S9
**批次 B（与 A 并发安全，同 executor 内分模块）**：S4 S5

> 实际可一次性 9 路全发；分 A/B 仅为标注 S4/S5 的 executor 内邻近关系，便于失败定位。

## 逐路 gate（orchestrator 执行，subagent 不得 commit）

每路 subagent 回报后：
1. 跑该路验证命令（见 prd 各项 `验证：`）+ `cargo check --workspace`（Rust 路）/ `pnpm -C packages/app-web exec tsc --noEmit`（S7）。
2. 绿 → `git add -A && git commit -m "<type>(<scope>): <中文>"`，一路一 commit。
   - 建议 type/scope：S1 `chore(agent-protocol): 删除 compat 死代码`；S2 `fix(application): 堵 routine/artifact 吞错与 panic`；S3 `fix(contracts): 修 MountCapability 变体漂移`；S4 `fix(executor): codex_bridge spawn 绑 cancel_token`；S5 `refactor(executor): MCP 连接池 + result→text 去重`；S6 `fix(first-party-plugins): authorize gate`；S7 `refactor(app-web): 去重 formatter/CapabilityDirective/JsonValue`；S8 `refactor(infra): 合并 db_err helper`；S9 `fix(api): relay registry 锁加固`。
3. 失败 → 一次定向修复；仍失败回退该路、journal 记录原因、跳过，不阻塞其余路。

## 完成判据

prd 的 Acceptance Criteria 全勾 + 9 路各自 commit（或被记录回退）。完成后回 parent 标记 W1 done，进 Wave 2。
