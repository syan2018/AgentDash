# Physical Dependency Contract

Date: 2026-06-25

## Purpose

本文件钉死 `agentdash-application` 拆分后的目标 Cargo graph。后续 crates-first 搬迁、编译修复和 check agents 都按这里判定依赖是否正确；临时编译红灯可以存在，但不能通过新增反向 crate dependency 或旧路径兼容壳解决。

## Target Cargo Graph

```text
agentdash-api / agentdash-local / agentdash-mcp
  -> agentdash-application

agentdash-application
  -> agentdash-application-agentrun
  -> agentdash-application-lifecycle
  -> agentdash-application-runtime-session
  -> agentdash-application-runtime-gateway
  -> agentdash-application-vfs
  -> agentdash-application-ports

agentdash-application-{agentrun,lifecycle,runtime-session,runtime-gateway,vfs}
  -> agentdash-application-ports

agentdash-application-ports
  -> agentdash-domain / agentdash-spi / agentdash-agent-protocol / agentdash-agent-types
```

`agentdash-application` 是 composition/umbrella crate：它可以依赖所有 implementation crates 并负责对旧 application consumers 提供收束后的 facade，但不拥有已抽出模块的业务实现。

## Crate Dependency Matrix

| Crate | May depend on | Must not depend on |
| --- | --- | --- |
| `agentdash-application-ports` | `agentdash-domain`, `agentdash-spi`, `agentdash-agent-protocol`, `agentdash-agent-types`, shared protocol/value crates | any `agentdash-application-*` implementation crate, `agentdash-application`, API/local/MCP, repository sets, AppState, builders |
| `agentdash-application-vfs` | ports, domain, spi, protocol/value crates | application umbrella, runtime-session, agentrun, lifecycle, runtime-gateway, canvas/session/lifecycle owner providers |
| `agentdash-application-runtime-gateway` | ports, domain, spi, protocol/value crates | application umbrella, runtime-session, agentrun, lifecycle, VFS implementation, API/local/MCP |
| `agentdash-application-runtime-session` | ports, domain, spi, agent protocol/types, optional generic VFS crate | application umbrella, agentrun implementation, lifecycle implementation, runtime-gateway implementation, owner providers |
| `agentdash-application-agentrun` | ports, domain, spi, agent protocol/types, generic VFS crate | application umbrella, lifecycle implementation, runtime-session implementation, runtime-gateway implementation |
| `agentdash-application-lifecycle` | ports, domain, spi, agent protocol/types, generic VFS crate | application umbrella, agentrun implementation, runtime-session implementation, runtime-gateway implementation |
| `agentdash-application` | all application implementation crates, ports, domain/spi/protocol crates | API/local/MCP route code; new business implementation for extracted owners |
| `agentdash-api` / `agentdash-local` / `agentdash-mcp` | application facade crate, extracted implementation crates when they are explicit composition inputs, ports | direct imports of moved module internals through old application paths |

## Runtime Relations Expressed Through Ports

These runtime relations are valid, but they must not become direct Cargo implementation dependencies:

| Runtime relation | Contract location |
| --- | --- |
| Lifecycle creates RuntimeSession delivery | `agentdash-application-ports::runtime_session_delivery` |
| Lifecycle materializes AgentRun frame evidence | `agentdash-application-ports::agent_frame_materialization` |
| AgentRun commits/adopts RuntimeSession live runtime | `agentdash-application-ports::frame_launch_envelope`, `runtime_surface_adoption` |
| RuntimeSession asks AgentRun for launch envelope, mailbox, capability or hook target facts | `frame_launch_envelope`, `runtime_session_live` |
| RuntimeGateway queries current MCP/runtime surface | `runtime_gateway_mcp_surface`, `agent_run_surface` |
| AgentRun projects Lifecycle resource/VFS surface | `lifecycle_surface_projection` |
| VFS summary consumes runtime projection facts from API/local | `vfs_surface_runtime`, `vfs_materialization` |

Composition root wiring lives in `agentdash-application` or API bootstrap. Concrete adapters can implement ports in their owner crate, but consumers depend on the port trait, not the owner implementation crate.

## Forbidden Cargo Edges

The following edges are always incorrect after crates-first split:

```text
agentdash-application-ports -> agentdash-application-*
agentdash-application-vfs -> agentdash-application
agentdash-application-vfs -> agentdash-application-{runtime-session,agentrun,lifecycle,runtime-gateway}
agentdash-application-runtime-gateway -> agentdash-application
agentdash-application-runtime-gateway -> agentdash-application-{runtime-session,agentrun,lifecycle,vfs}
agentdash-application-runtime-session -> agentdash-application
agentdash-application-runtime-session -> agentdash-application-{agentrun,lifecycle}
agentdash-application-agentrun -> agentdash-application
agentdash-application-agentrun -> agentdash-application-lifecycle
agentdash-application-agentrun -> agentdash-application-runtime-session
agentdash-application-lifecycle -> agentdash-application
agentdash-application-lifecycle -> agentdash-application-agentrun
agentdash-application-lifecycle -> agentdash-application-runtime-session
```

If one of these appears during repair, the fix is one of:

- move DTO/trait/error to `agentdash-application-ports`
- move generic helper to the owner crate that truly owns it
- move concrete wiring to composition root
- delete stale facade/test/path

Do not add a compatibility module to preserve the forbidden edge.

## Owner Files Excluded From Generic VFS

`agentdash-application-vfs` must not absorb owner-specific files:

- `crates/agentdash-application/src/canvas/vfs_*`
- `crates/agentdash-application/src/lifecycle/vfs_*`
- `crates/agentdash-application/src/session/vfs_owner_providers.rs`
- `crates/agentdash-application/src/vfs_surface_resolver.rs`

These remain in owner/application adapter space unless a later dedicated task extracts the owner itself.

## Crates-First Rule

Round 5 may create every remaining target crate and move files before compilation is green. The checkpoint value is a fixed target Cargo graph plus owner-assigned compile errors.

Check agents should review against this file first. A red compile result is acceptable when blockers are classified by target crate and forbidden edge; a green result that preserves forbidden edges is not acceptable.
