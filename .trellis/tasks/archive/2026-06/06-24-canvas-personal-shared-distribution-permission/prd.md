# Canvas 个人与共用分发权限系统

## Goal

让 Canvas 从单一 Project 共享资产升级为具备个人归属、项目共用发布、只读使用、复制后独立编辑的可分发资产。

第一期目标是完成 Project 内闭环：用户可以创建和编辑自己的个人 Canvas，将其发布为项目共用 Canvas；项目成员可以使用共用 Canvas，但共用源保持只读；成员需要修改时将共用 Canvas 复制为自己的个人实例后继续编辑。

## User Value

- 个人可以先在自己的 Canvas 中探索和迭代，不影响项目内其他人。
- 稳定产物可以发布为项目共用 Canvas，成为团队可见的可运行资产。
- 项目成员可以安全使用共用 Canvas，而不会误改发布源。
- 项目成员可以把共用 Canvas 复制成个人副本，在独立实例里继续试验和定制。
- Canvas 后续进入 Shared Library / Marketplace 时有明确的来源、版本和安装语义基础。

## Confirmed Facts

- 当前 `Canvas` 领域实体只有 `project_id`、`mount_id`、标题、文件、binding 等项目级字段，没有 owner、scope、published source 或 clone lineage。
- 当前 Canvas API 权限完全回到 Project 权限：读取要求 Project view，创建、更新、删除、发布为插件要求 Project edit。
- 当前 Canvas VFS mount 默认暴露 read / write / list / search，provider 也提供 write / delete / rename，因此只读语义不能只停留在 API 或前端。
- 前端 Assets 页当前把 Canvas 标为本地类目，不进入资源市场；`CanvasCategoryPanel` 已记录后端尚未提供 duplicate API。
- Shared Library 已有 publish/install/source-status 主干，但 `LibraryAssetType` 还没有 `canvas_template`。现有“Canvas 发布为插件”生成 packaged extension artifact，语义是运行面板分发，不等价于可复制编辑的 Canvas 源模板。
- Project 模板与 clone 已经采用“可查看/可使用源，但私有化修改通过 clone 副本完成”的产品模式，本任务复用同一类语义。
- 进行中的 Canvas VFS/runtime binding 收束任务会统一 `canvas_id`、`canvas_mount_id`、`vfs_mount_id`、`canvas://...`、`canvas:{...}` 等身份词汇，本任务应以该收束后的词汇为边界。
- 进行中的 AgentFrame 与 Canvas projection 收束任务会修复 Canvas present/expose 后 runtime surface 与前端 workspace 读取同一 frame 的问题，本任务的只读 runtime surface 裁切应接在同一投影路径上。

## Product Decisions

- MVP 只实现 Project 内分发闭环：个人 Canvas、项目共用 Canvas、发布、取消发布、复制为个人实例。
- 共用 Canvas 的源实例对普通项目成员只读；编辑必须复制为个人 Canvas。
- 第一期开启“共用 Canvas 由发布者/项目 owner 管理”的规则，不引入多人协同编辑和 Canvas 级 collaborator 列表。
- “发布到项目共用”和“发布为插件”保留为两条不同产品路径：前者保留 Canvas 源文件和复制编辑能力，后者生成 Extension package 运行面板。
- Shared Library `canvas_template` 作为后续阶段，不阻塞 MVP。

## Requirements

### Canvas Ownership And Scope

- Canvas 必须有明确归属：
  - `personal`：归属于当前用户的个人 Canvas，owner 可编辑。
  - `project`：发布到项目共用区的 Canvas，项目成员可查看和使用，源内容默认只读。
- Canvas 响应 DTO 必须返回当前用户对该 Canvas 的 effective access，例如 owner editable、project read-only、project manage。
- Canvas 列表必须能区分“我的 Canvas”和“项目共用 Canvas”，前端资产页据此展示不同主操作。
- 新建 Canvas 默认创建为当前用户的个人 Canvas。

### Publish To Project Shared Canvas

- 个人 Canvas owner 可以将其发布为项目共用 Canvas。
- 发布时后端复制当前 Canvas 的 title、description、entry_file、sandbox_config、files、bindings，生成独立的项目共用 Canvas 源记录。
- 发布记录必须保留 lineage：项目共用 Canvas 能指向其来源个人 Canvas；个人 Canvas 能识别是否已有对应共用发布。
- 更新发布必须走显式 publish/update-publish 流程，不能通过普通 update 直接修改项目共用源。
- 项目 owner 可以管理项目共用 Canvas 的可见状态，至少支持取消发布或删除共用记录。

### Read-Only Shared Usage

- 项目成员具备 Project view 时可以读取、预览、present 项目共用 Canvas。
- 项目共用 Canvas 进入 session runtime surface 时，其 VFS mount 必须只暴露 read / list / search，不暴露 write/edit 能力。
- 项目共用 Canvas 的 WorkspaceModule descriptor 必须裁切 mutation operation；`canvas.bind_data` 等会改变 Canvas 源的操作只对 editable Canvas 暴露。
- HTTP update/delete、VFS write/delete/rename、WorkspaceModule mutation 三条路径必须一致拒绝普通成员修改项目共用源。

### Copy To Personal Canvas

- 项目成员可以将项目共用 Canvas 复制为自己的个人 Canvas。
- 复制生成新的 Canvas UUID 和新的 `canvas_mount_id`，并复制 files / bindings / sandbox_config / entry_file 等 authoring 内容。
- 复制后的个人 Canvas 与源共用 Canvas 解耦，owner 可独立编辑，不会影响项目共用源。
- 复制结果必须记录 `cloned_from_canvas_id` 或等价 lineage，供后续展示来源与更新提示。

### API And Frontend

- 后端提供明确的 publish、copy、unpublish 或等价命令接口，避免用普通 update 表达发布语义。
- 前端 Canvas 资产页提供“我的”和“项目共用”两个视图或等价分组。
- 个人 Canvas 的主操作是编辑、发布、删除。
- 项目共用 Canvas 的主操作是打开/预览、复制为我的 Canvas；管理者额外看到取消发布或删除共用记录。
- 只读 Canvas 的编辑控件、文件写入入口、binding mutation 入口必须呈现为不可编辑状态或不出现。

### Migration And Contracts

- 数据库 migration 必须为既有 Canvas 赋予合理初始归属。预研阶段推荐把既有 Project Canvas 迁为项目共用 Canvas 或当前 personal 模式用户的个人 Canvas，具体取决于实现时当前认证上下文可用性。
- API contract、generated TypeScript、前端类型 facade 同步新增 scope/access/lineage 字段。
- 不保留旧字段别名或兼容双写；项目未上线，直接收束到正确契约。

### Spec Updates

- 更新 Canvas / VFS / WorkspaceModule / Shared Library 相关 spec，记录 Canvas ownership、project shared source、copy lineage、read-only VFS projection 的正向语义。
- 更新 Canvas system skill，使 Agent 了解只读共用 Canvas 需要复制后编辑。

## Acceptance Criteria

- [ ] Canvas domain / repository / migration 支持 owner、scope、publish/copy lineage，并有自动化测试覆盖默认创建、发布、复制、取消发布语义。
- [ ] 既有 Canvas 数据迁移后仍可被项目读取，且新字段满足最终模型约束。
- [ ] `GET /projects/{project_id}/canvases` 或新列表 API 能区分 personal/project shared，并返回当前用户 effective access。
- [ ] 新建 Canvas 默认归属于当前用户个人空间，owner 可以编辑。
- [ ] 个人 Canvas owner 可以发布为项目共用 Canvas；发布后项目成员可以查看和 present。
- [ ] 普通项目成员不能通过 HTTP update/delete 修改项目共用 Canvas。
- [ ] 项目共用 Canvas 进入 runtime surface 时 VFS mount 不包含 write capability，VFS write/delete/rename 会被拒绝。
- [ ] 项目共用 Canvas 的 WorkspaceModule descriptor 不暴露会修改 Canvas 源的 operation。
- [ ] 项目成员可以复制项目共用 Canvas 为个人 Canvas；复制后新实例具备独立 UUID、独立 `canvas_mount_id`、独立 files/bindings，并记录来源。
- [ ] 复制后的个人 Canvas 可编辑，修改不会影响源项目共用 Canvas。
- [ ] 前端 Canvas 资产页能清晰展示个人与项目共用 Canvas，并按 access 显示编辑、发布、复制、只读预览等操作。
- [ ] Canvas runtime preview/present 对个人 Canvas 和项目共用 Canvas 均可用；只读状态不影响读取和展示。
- [ ] Rust contract 生成和前端类型检查通过。
- [ ] 关键后端测试、前端 Canvas 面板测试、workspace module/VFS 相关测试通过。
- [ ] 相关 Trellis spec 和 Canvas skill 已更新。

## Dependencies And Ordering

- 本任务应在 Canvas identity / VFS mount id 收束后实施，或在同一实现窗口内以其最终约定为准，避免重新引入 `canvas_id` / mount id 双义字段。
- 本任务应依赖 AgentFrame Canvas projection 收束后的 canonical runtime surface 路径，read-only VFS 裁切必须进入同一 projector/update service。

## Out Of Scope For MVP

- 跨 Project / Marketplace 分发的 `canvas_template` asset type。
- 多人协作编辑同一个 Canvas 源。
- Canvas 级用户/用户组 ACL。
- 自动同步个人副本与项目共用源的更新。
- 发布版本历史、diff viewer、回滚 UI。
- 改造现有“发布为插件”能力；该路径只需要在 UI 文案上和“发布到项目共用”区分。

## Notes

- 推荐 Review Gate：实现前确认 MVP 范围保持在 Project 内分发闭环，Shared Library `canvas_template` 另开后续任务。
