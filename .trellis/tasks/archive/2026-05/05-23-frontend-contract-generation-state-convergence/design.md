# 前后端契约生成与前端状态收拢 Design

## Boundary

本任务跨 Rust contract generation 与 React frontend state，但每个实施切片应保持小而可验证。

## Contract Generation

优先将高漂移 DTO 纳入生成：

1. Workflow contract / lifecycle activity；
2. Session DTO / runtime surface；
3. VFS surface / mount DTO；
4. Shared Library / MCP Preset / ProjectAgent。

生成链路应支持 check mode，防止生成文件 drift。

## Frontend State

目标结构：

```text
features/<domain>/
  api.ts
  types.ts
  normalize.ts
  reducer.ts
  selectors.ts
  store.ts
  components/
```

stream hook 拆为：

```text
stream transport -> event normalizer -> reducer -> React hook
```

## Migration Strategy

先选一个 DTO 域和一个 stream/store 切片，证明生成与前端拆分模式，再扩大覆盖。

## Spec Update

更新 frontend type-safety、state-management、hook-guidelines，并在 cross-layer spec 记录生成 DTO 事实源。
