# WI-05 Canvas Interaction Replacement

Status: done

Depends On: WI-02、WI-04

## Scope

- Canvas authoring/source/layout 直接由 canonical V1 InteractionDefinition revision + immutable SourceBundle 承载。
- Personal publish/Project unpublish/copy-to-personal exact revision lineage 与 archive retention。
- definition resource slots、instance/attachment authorized binding 与只读 `bindings/*` VFS projection。
- Extension promotion 固定 exact definition revision/source digest。
- instance/attachment/runtime binding 建立，authoring/runtime module/presentation identity 分离。
- renderer lease、reload、multi-tab 与 presentation state 分层。
- 删除旧 Canvas aggregate/runtime snapshot tables、repositories、routes、DTO 与 frontend consumers。
- AgentFrame 只保留 effective attachment/surface projection。
- fixtures、seed、examples 和 tests 直接按最终模型重建，不回填旧数据。

## Exit Criteria

- Canvas source、shared state、Agent attachment、tab/renderer 不再混成同一对象。
- reload 从 canonical instance state 初始化。
- 既有 Canvas CRUD、publish/copy/unpublish、effective access、VFS/data binding 与 promotion 语义完整落在新模型。
- 旧五张 Canvas/runtime state 表、compatibility decoder、Session authority 与重复 state source 被删除。

## Validation

- clean database migration、SourceBundle/lineage/resource binding、instance pinning/reload/multi-tab tests。
- publish/copy/unpublish/shared-read-only-VFS/promotion end-to-end tests。
- 旧 table/route/contract/repository residual scan。
- Agent projection 与 WorkspacePanel integration tests。
