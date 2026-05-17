# Workspace 列表与编辑流程重整

## Goal

重整 Project Workspace 列表、详情与创建流程的心智模型：Workspace 作为逻辑身份，Binding 作为由 backend inventory 匹配/确认生成的运行时落点；弱化 backend/root 手填主路径，补齐 candidates、resolution、advanced binding 编辑体验。

## Background

当前 Workspace 列表和编辑 drawer 仍带有明显“backend 目录绑定管理器”的痕迹：

- 创建入口优先要求用户选择 backend + root_ref 再自动识别。
- Workspace 详情里 identity JSON、binding status、detected_facts 等高级字段过早暴露。
- Project default workspace 与 Workspace default binding 在 UI 心智上容易混在一起。
- backend inventory / candidate / auto binding sync 已经出现，但 Workspace 列表尚未围绕这些派生结果组织。

新的目标是让 Project Workspace 页面表达“这个 Project 需要哪些 logical workspace”，而不是让用户维护一堆 backend 路径。

## Requirements

### R1. Workspace 列表以逻辑身份为中心

列表卡片主信息应展示：

- Workspace 名称与 identity kind。
- identity 摘要，例如 Git repo key / branch policy、P4 stream/client、LocalDir path key。
- 是否为 Project default workspace。
- bindings 总数、在线可用数量。
- 当前 runtime resolution 结果：选中了哪个 backend/root，或为什么无法解析。
- warnings，例如无授权 backend、无匹配 binding、backend 离线、identity 不匹配。

列表主操作只保留：

- 设为 Project 默认 Workspace。
- 打开详情。
- archive/delete。

### R2. Workspace 详情分层展示

详情或 drawer 应拆成：

1. `Identity`：维护 logical workspace contract；JSON 仅放高级折叠区。
2. `Resolution`：展示当前解析结果、选择理由、不可用诊断。
3. `Bindings`：展示已确认的 backend/root 落点，支持设 Workspace default binding、调整 priority、detach、refresh verify。
4. `Candidates`：展示 backend inventory 中匹配/疑似匹配当前 Workspace 的候选，可确认生成 binding。

### R3. 创建流程拆成三种入口

主流程：

- 从 unmatched candidate 创建 Workspace：系统预填 identity、名称、初始 binding，用户确认。
- 创建空 logical Workspace：用户先填写 Git/P4/LocalDir identity，等待 backend inventory 自动匹配。

高级流程：

- 从 backend/root 手动 detect 创建：保留能力，但放到高级入口，不作为默认新建按钮心智。

### R4. 区分两个 default 概念

UI 必须明确区分：

- Project default workspace：Project 默认使用哪个 logical Workspace。
- Workspace default binding：某个 logical Workspace 默认落到哪个 backend/root。

这两个动作不能使用含糊的同一文案或同一视觉层级。

### R5. 弱化手工 binding 编辑

手工输入 backend/root、编辑 detected_facts、修改 binding status 仅作为高级操作保留。主路径优先使用 backend inventory candidate 和自动 sync 产生 binding。

### R6. 保持与 ProjectBackendAccess / backend inventory 的主链路一致

Workspace 列表和详情不得再暗示 Project 能任意选择 backend/root。可用 binding 与 candidate 必须来自当前 Project 已被授权的 backend inventory。

## Acceptance Criteria

- [ ] Workspace 列表卡片展示 logical identity 摘要、Project default 状态、binding 可用性和 runtime resolution 诊断。
- [ ] Workspace 详情拆分 Identity / Resolution / Bindings / Candidates 四个区域。
- [ ] 新建 Workspace 默认入口不再优先引导用户填写 backend + root_ref。
- [ ] Candidate 可作为创建 Workspace 的主入口，创建前展示预填 identity 和初始 binding。
- [ ] 手动 backend/root detect 入口移动到高级区域。
- [ ] Project default workspace 与 Workspace default binding 的文案和交互明确区分。
- [ ] 无授权 backend、无匹配 binding、backend 离线、identity 不匹配时给出明确诊断，不静默 fallback。
- [ ] 前端类型检查、lint、build/test 通过。

## Notes

- 父任务：`05-17-project-backend-workspace-routing-design`。
- 相关但不同的后续任务：Backend 设置页承载 backend owner 授权 Project 的入口迁移。
