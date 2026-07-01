# Runtime context discovery 单入口收束设计

## Design Goal

把 `FrameLaunchEnvelope` 整理为语义清晰的 launch projection 边界，并把 runtime context discovery 从多个启动路径的局部副作用，收束为 launch-time 的显式 projection。所有 connector prompt 需要的动态发现上下文，都应从最终 `FrameLaunchSurface` 派生一次，再进入 `FrameLaunchIntent` / `LaunchPlan` / `ContextFrame`。

## Envelope Boundary

`FrameLaunchEnvelope` 顶层应表达“这次 launch 的闭包事实”，而不是把各阶段零件平铺暴露。推荐形态：

```rust
pub struct FrameLaunchEnvelope {
    pub frame: FrameLaunchFrameRef,
    pub command: FrameLaunchCommandIntent,
    pub runtime: FrameLaunchRuntimeSurface,
    pub context: FrameLaunchContextProjection,
    pub diagnostics: FrameLaunchDiagnostics,
}

pub struct FrameLaunchFrameRef {
    pub surface: FrameRuntimeSurface,
    pub pending_frame: Option<AgentFrame>,
}

pub struct FrameLaunchCommandIntent {
    pub input: Option<Vec<UserInputBlock>>,
    pub environment_variables: HashMap<String, String>,
    pub identity: Option<AuthIdentity>,
    pub terminal_hook_effect_binding: Option<TerminalHookEffectBinding>,
}

pub struct FrameLaunchRuntimeSurface {
    pub surface_draft: FrameSurfaceDraft,
    pub launch_surface: FrameLaunchSurface,
    pub working_directory: PathBuf,
    pub runtime_backend_anchor: RuntimeBackendAnchor,
    pub base_capability_state: Option<CapabilityState>,
}

pub struct FrameLaunchContextProjection {
    pub context_bundle: Option<SessionContextBundle>,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub discovered_memory: MemoryDiscoveryOutput,
}

pub struct FrameLaunchDiagnostics {
    pub resolution_trace: LaunchResolutionTrace,
}
```

The exact type names may differ, but the boundary should preserve these roles:

- command: request intent
- runtime: closed execution surface
- context: prompt/context discovery projection
- diagnostics: trace/debug projection
- frame: persisted frame refs

## Proposed Shape

在 envelope runtime surface 闭包后，新增或收束一个 application-level orchestration：

```rust
pub struct LaunchContextDiscoveryInput<'a> {
    pub vfs_service: &'a VfsService,
    pub runtime: &'a FrameLaunchRuntimeSurface,
    pub identity: Option<&'a AuthIdentity>,
    pub extra_skill_dirs: &'a [PathBuf],
    pub skill_discovery_providers: &'a [Arc<dyn SkillDiscoveryProvider>],
    pub memory_discovery_providers: &'a [Arc<dyn MemoryDiscoveryProvider>],
    pub diagnostics_label: &'static str,
}

pub struct LaunchContextDiscoveryOutput {
    pub session_capabilities: SessionBaselineCapabilities,
    pub discovered_guidelines: Vec<DiscoveredGuideline>,
    pub discovered_memory: MemoryDiscoveryOutput,
}
```

调用时机：

```text
frame route compose / existing surface
  -> build or read FrameSurfaceDraft
  -> close_frame_launch_surface
  -> FrameLaunchRuntimeSurface
  -> LaunchContextDiscovery::derive(...)
  -> FrameLaunchContextProjection
  -> FrameLaunchEnvelope { command, runtime, context, diagnostics, frame }
  -> LaunchPlan
  -> TurnPreparer context frames
```

## VFS File Discovery Ownership

`agentdash-application-vfs::mount_file_discovery` 是唯一拥有 mount scanning 语义的位置：

- mount 是否自动扫描
- metadata allow/deny 策略
- path normalization
- read/list 调用和 identity 透传
- recursive traversal
- max file size diagnostics
- empty content filtering

Guideline、Memory、Skill 只保留 adapter：

- Guideline adapter: `DiscoveredMountFile -> DiscoveredGuideline`
- Memory adapter: `MemoryDiscoveryVfsRule -> MemoryDiscoveryVfsFile`
- Skill adapter: `SkillDiscoveryVfsRule -> SkillDiscoveryVfsFile`

## Route Coverage

所有 `FrameLaunchEnvelope` route 都应消费同一个 discovery output：

| Route | Discovery source |
| --- | --- |
| ProjectAgent owner compose | final launch surface VFS |
| LifecycleNode compose | final launch surface VFS |
| ExistingSurface | persisted frame launch surface VFS |
| Companion modifier | child launch surface VFS after slice/modifier |
| Runtime capability refresh | active runtime VFS through the same VFS file discovery adapters |

## ContextFrame Contract

- `identity` 只承载 stable identity fragments。
- `system_guidelines` 承载 user preferences 与 project guidelines。
- `memory_context` 承载 memory inventory。
- Capability/skill/tool deltas 保持 discovered inventory phase。

Delivery order remains:

```text
identity #10
system_guidelines #20
compaction_summary #30
assignment_context #40
capability_state_delta #50
memory_context #60
```

## Tests

Required test layers:

- VFS unit tests for generic mount file discovery policy.
- Adapter tests for guideline/memory/skill rule conversion.
- Frame construction test for `AGENTS.md -> FrameLaunchIntent.discovered_guidelines`.
- Session launch test for `FrameLaunchIntent.discovered_guidelines -> system_guidelines ContextFrame`.
- ExistingSurface launch regression test.
- Frontend parser/render test for visible `system_guidelines` frame.
