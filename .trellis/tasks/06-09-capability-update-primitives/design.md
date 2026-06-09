# Design — Capability Update Primitives

> 事实基础见 [research/01-current-capability-model.md](research/01-current-capability-model.md)。本文定义原语模型、决策、维度归类表与子任务边界。

## 1. 概念模型：base ⊕ modifier，按 policy 解析

一个 revision 上某维度的有效值 = `resolve(base, [modifiers], policy)`：

- **base（原始状态 / 声明式真值）**：来自 ProjectAgent preset 的声明，materialized 进 `AgentFrame.effective_capability_json`（= 序列化的 `CapabilityState`）。声明式、幂等、可整体替换。
- **modifier（编辑修饰符 / 运行时增量）**：`RuntimeCapabilityTransition`（declarations + effects），由 workflow / permission grant 在运行时产生，经重放叠加到 base 上。
- **policy（积累规则）**：决定 modifier 如何与 base/前序 modifier 合并。

这一层**已在代码里半成型存在**（见 research §2），本任务是把它补全为全维度唯一面 + 显式化 policy，而非另起炉灶。

## 2. 原语词汇（「更新」与「标识」）

### 2.1 标识原语（已存在，复用）
- `CapabilityDimensionKey`：维度标识。新增 `skill` / `workspace_module` 两个常量补齐 6 维。
- `CapabilityArtifactSource { kind }`：来源标识。现有 `workflow` / `permission_grant`，**补 `preset`**（声明式 base 的来源）。
- `declaration_type` / `effect_type`：操作类型标识。

### 2.2 更新原语（部分存在，需显式化）
- **Declaration**（意图）：`CapabilityDeclarationRecord`。携带 Add/Remove 这类增量意图（如 `ToolCapabilityDirective::Add/Remove`）。
- **Effect**（已编译的状态 diff）：`RuntimeCapabilityEffectRecord`。重放时按维度 module 改写 state。
- **Transition**：一次更新 = `{ declarations, effects }`。
- **AccumulationPolicy（新增原语，核心交付）**：见 §3。

> 用户语言映射：**"grant 型会积累的 modifier"** = Accumulate policy 下的 effect 流（vfs overlay / canvas mount）；**"随状态即用即弃的 modifier"** = Ephemeral policy；**"原始状态与修饰符分开"** = base(effective_capability_json) 与 transition 重放两层。

## 3. AccumulationPolicy（本任务核心新增）

```rust
// 拟放 spi/session_persistence.rs，与现有 Capability* 标识原语同层（决策 DD）
pub enum AccumulationPolicy {
    /// 声明式整体替换：base 由 config 全量覆盖；清空 = 回默认/全集；modifier 不跨 revision 累积。
    Replace,
    /// 累积：modifier 跨 revision 叠加，直到显式 Revoke / 清空源。
    Accumulate,
    /// 即用即弃：modifier 仅当前 revision 有效，不写入下一 revision 的 base。
    Ephemeral,
}
```

每个 `CapabilityDimensionModule` 新增 `fn policy(&self) -> AccumulationPolicy`。重放/帧构建统一查询 policy，不再各字段硬编码 merge。

## 4. 6 维度归类表（验收 1/4 的依据）

| 维度 | Policy | base 投影来源 | modifier 来源 | 现状→目标 |
|---|---|---|---|---|
| tool.capabilities/clusters | Replace（effect 层）| preset.capability_directives → effective_capability_json | permission grant declarations（Add/Remove 累积于声明层）| canonical，仅标注 policy |
| tool.mcp_servers | Replace | preset.mcp_preset_keys → mcp_surface_json | — | canonical，仅标注 policy |
| vfs.active | Accumulate | preset.vfs_access_grants → vfs_surface_json | mount overlay/ops effects + canvas mount | canonical，仅标注 policy |
| companion.agents | Replace | preset.allowed_companions → effective_capability_json | — | **退役 resolver 直接赋值旁路 → 收口 base 投影** |
| skill.skills | Replace | preset.skill_asset_keys → effective_capability_json | — | **退役 frame_builder carry-forward 旁路 → 收口 base 投影** |
| workspace_module | Replace | preset.visible_workspace_module_refs → effective_capability_json（mode 三态）| （预留 Accumulate 运行时 grant，本期不开）| **退役 visible_workspace_module_refs_json + frame_construction 直接赋值旁路 → 收口 base 投影，修 bug** |

> 注：canvas mount（`visible_canvas_mount_ids_json`）作为 vfs 维度下 Accumulate 的既有样本保留不动；它本就是"运行时累积的 grant"，policy 归类为 Accumulate 即可，无需改语义。

## 5. 三态原语

config 字段 `Option<Vec<T>>` 的三态在全链路保真，禁止 `unwrap_or_default` 提前抹平：

| 语义 | config 表示 | 解析结果 |
|---|---|---|
| `Unspecified` 未声明 | 字段缺省 `None` | 继承上一 revision base（Replace 维度即沿用上次声明）|
| `Cleared` 显式清空 | `Some([])` | 回默认（Replace 维度 → 全集 / mode=All），**不继承旧值** |
| `Allowlist` 受限 | `Some([..])` | 写入受限集 |

对 workspace_module，三态天然由 `WorkspaceModuleDimension.mode` 承载：`mode=All`（Unspecified 或 Cleared）/ `mode=Allowlist`（受限）。`Cleared` 与 `Unspecified` 在 Replace 语义下**对外等价**（都=全集），区别仅在"是否覆盖上一版声明"——而 base 走 effective_capability_json 每 revision 由 config 重新投影，因此 `Cleared` 自然产出 All、`Unspecified` 沿用上次，bug 消失。

## 6. 关键决策（钉死）

| ID | 决策 | 取舍 |
|---|---|---|
| **DA** | 不在 AgentFrame 上物理拆 base/modifier 字段。base 仍存 `effective_capability_json`，modifier 仍走现有 transition 表（`AgentFrameTransitionRecord`）。 | 零新表、零迁移风险；分层是**逻辑**而非物理。 |
| **DB** | workspace_module 声明式 allowlist 收口到 base `CapabilityState.workspace_module`（经 effective_capability_json），由 `capability_state_to_frame_surfaces` 序列化、`project_capability_state_from_frame` 还原。退役 `visible_workspace_module_refs_json` 旁路与 frame_construction 直接赋值。 | 最小改动且天然修 bug；mode 字段承载三态。`visible_workspace_module_refs_json` DB 列保留为"运行时 Accumulate grant 预留"，本期写入逻辑删除（不删列，避免迁移；列变为始终 NULL，并在 doc 标注预留）。 |
| **DC** | skill/companion 退役旁路采用**统一 base 投影**而非全量事件溯源：声明式真值经 resolver 产出后写入 base CapabilityState（companion 已是 resolver 产物，skill 改为同样从 base 投影而非 frame_builder carry-forward）。两者 policy=Replace，effect 通道保留给未来运行时增量。 | 不对最敏感的能力门做高风险事件溯源大改；收敛点是"单一 base 投影来源"，与 tool 一致。 |
| **DD** | `AccumulationPolicy` 放 `agentdash-spi/src/session_persistence.rs`，与 `CapabilityDimensionKey`/`CapabilityArtifactSource` 同层。 | 标识与策略原语聚拢一处；application 的 module 实现引用之。 |
| **DE** | composer 层移除 `unwrap_or_default`，改传 `Option<Vec<T>>` 三态直达 base 投影；frame_builder 删除 workspace_module 混合 match 臂。 | 三态保真的落点。 |

## 7. 数据流（目标态）

```
preset(Option<Vec<T>> 三态)
   │  resolver / base 投影（按维度 policy=Replace 整体替换；mode 承载 ws_module 三态）
   ▼
base CapabilityState ──serialize──► effective_capability_json (+ vfs/mcp surface)
   │
   │  运行时 RuntimeCapabilityTransition（declarations/effects，policy=Accumulate/Ephemeral）
   ▼  replay_runtime_capability_transition(base, transition)
有效 CapabilityState ──► tools 过滤 / VFS mount / companion 名单
```

frame_builder 不再 per-field 手写 merge：base 经统一投影写入 effective_capability_json（标准 or_else 继承仍适用于 Unspecified），旁路字段退役。

## 8. 子任务边界（2 child）

### Child A — 能力原语 + workspace_module 收口（含前端）
> slug `workspace-module-base-converge`；合并原 4-child 的 原语/workspace_module/前端 三块为一条纵切。
- **原语**：spi 新增 `AccumulationPolicy` + `CAPABILITY_DIMENSION_SKILL/WORKSPACE_MODULE` 常量 + `CapabilityArtifactSource::preset()`；`CapabilityDimensionModule` 加 `policy()`，4 module 标注。纯加性先落。
- **workspace_module 收口**：allowlist 不再写 `visible_workspace_module_refs_json`，改投影进 base CapabilityState.workspace_module（mode 三态）；删除 frame_builder 混合 match 臂 + `with_visible_workspace_module_refs`、frame_construction L363-374 直接赋值、composer workspace_module 路径 `unwrap_or_default`。
- **前端**：preset-editor 清空写显式空（→All）、picker 三态；契约导出（如需）；docs 能力原语 + Workspace Module 节收尾。
- 回归：set→clear→All；set→保持→Allowlist；新 agent→All；UI 端到端。
- 取代 PR #45 Child 4 临时语义。**核心交付，可独立于 Child B 交付。**

### Child B — skill / companion 旁路退役
- skill：删 frame_builder carry-forward（L58-61），改从 base 投影；companion：确认 resolver 产物即 base，去除任何二次直接赋值分叉。policy=Replace 归类落地。
- **风险中**（触碰能力门投影），需细致回归；依赖 Child A 的 policy 词汇。

> 顺序：Child A（含原语，可独立交付修 bug）→ Child B（依赖 A 的 policy 词汇）。

## 9. 兼容与回滚

- 无新表、无破坏性迁移（DA/DB）。`visible_workspace_module_refs_json` 列保留（变 NULL），可回滚到旧写入逻辑。
- effective_capability_json 是既有列，workspace_module 维度本就在 CapabilityState 内（`#[serde(default)]`），旧 frame 反序列化默认 mode=All，向后兼容。
- 每个 child 独立 `cargo build --workspace` + 测试 + contracts:check + app-web typecheck 通过方可 archive。
