# 部署基准方案与发布流程执行计划

## Current State

- 已创建分支：`codex/deployment-baseline`。
- 已新增文档：`docs/deployment/deployment-baseline.md`。
- 已创建 Trellis 任务：`.trellis/tasks/05-29-deployment-baseline`。
- 当前任务保持 `planning` 状态。

## Suggested Implementation Slices

### Coordination Model

父任务保持规划和集成协调，不直接包揽所有实现。其它机器上的 Agent 优先认领以下子任务：

| 子任务 | 推荐认领对象 | 起步文件 |
| --- | --- | --- |
| `05-29-deployment-cloud-primitives` | 后端/部署 Agent | `.trellis/tasks/05-29-deployment-cloud-primitives/implement.md` |
| `05-29-deployment-compose-runbook` | 部署/DevOps Agent | `.trellis/tasks/05-29-deployment-compose-runbook/implement.md` |
| `05-29-deployment-desktop-targeting` | 桌面/前端 Agent | `.trellis/tasks/05-29-deployment-desktop-targeting/implement.md` |

当前会话继续维护：

- `docs/deployment/deployment-baseline.md`。
- 父任务 `prd.md` / `design.md` / `implement.md`。
- 子任务之间的字段命名、依赖关系和交接说明。
- 仓库级部署环境与发布链路：
  - release build / image build 入口。
  - 版本注入。
  - `deploy/` 目录规划。
  - CI / release workflow 草案。
  - 产物发布与 compatibility matrix。

### Original Slice List

1. 文档与部署契约收束
   - 完善 `docs/deployment/deployment-baseline.md`。
   - 补充云端配置契约。
   - 明确 Compose/Kubernetes 映射关系。
   - 明确桌面端发布策略。

2. 云端版本与发现端点
   - 增加 `/api/version`。
   - 增加 `/.well-known/agentdash`。
   - 定义 server / desktop / local runtime 版本字段。
   - 为桌面端 discovery 做最小测试。

3. 云端运行入口
   - 梳理 `agentdash-server serve` 当前行为。
   - 拆出或新增 `agentdash-server migrate`。
   - 新增 `agentdash-server doctor`。
   - 保留启动时 schema readiness check。

4. Docker Compose 基准
   - 新增 `deploy/compose/docker-compose.yml`。
   - 新增 `deploy/compose/.env.example`。
   - 新增 cloud Dockerfile。
   - 建立 app、migrate、postgres、reverse-proxy 的最小运行链路。

5. 更新与恢复 runbook
   - 新增 Compose upgrade 文档。
   - 新增 backup / restore 脚本或命令说明。
   - 明确 rollback 边界。

6. 桌面端服务器配置
   - 将构建时 default API origin 作为默认值而非唯一值。
   - 增加运行时服务器配置持久化。
   - 接入 discovery endpoint。
   - 增加版本兼容提示。

7. 仓库部署环境与发布链路
   - 规划 release build 命令。
   - 规划 cloud image build 命令。
   - 规划版本注入字段来源。
   - 规划 `deploy/` 目录结构。
   - 规划 CI / release workflow。
   - 明确 cloud、Compose、desktop 子任务如何消费这些产物。

## Validation Plan

按具体切片选择验证命令：

```bash
pnpm run backend:check
pnpm run backend:test
pnpm run frontend:check
pnpm run desktop:check
pnpm run frontend:build
pnpm run desktop:bundle
```

部署脚本相关切片需要增加：

```bash
docker compose config
docker compose up -d
docker compose run --rm migrate
```

## Review Gates

- 启动实现前确认第一轮优先切片。
- 涉及数据库 migration 子命令前确认与现有启动时 migration 的关系。
- 涉及桌面端发布语义前确认 `builtin`、`external`、`sidecar` 的最终产品含义。
- 涉及 Compose reverse proxy 前确认是否内置 Caddy/Nginx，或只提供接入样例。

## Initial Recommendation

第一轮建议先做“云端版本与发现端点”以及“部署基准文档收束”。原因是这两项能先固定云端和桌面端之间的部署契约，后续 Compose、桌面配置和 K8s 映射都可以围绕该契约推进。

## Recommended Parallel Start

如果其它机器上的 Agent 现在可以并行工作，推荐这样分配：

1. 那边 Agent A 先做 `05-29-deployment-cloud-primitives`，重点确认 `/api/version`、`/.well-known/agentdash` 和配置契约。
2. 那边 Agent B 做 `05-29-deployment-compose-runbook`，先基于目标契约产出 Compose scaffold 和 runbook 草案，不等待所有 endpoint 实现。
3. 那边 Agent C 做 `05-29-deployment-desktop-targeting`，先设计运行时 server origin 状态流和发布语义，使用 mock discovery schema。
4. 当前会话维护父任务、主文档和仓库部署链路，等 A/B/C 的结论回来后做一次契约合并。
