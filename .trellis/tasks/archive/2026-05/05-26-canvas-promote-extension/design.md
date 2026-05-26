# Design

## Mapping

```text
Canvas.files -> package assets
Canvas.entry_file -> workspace_tabs[].renderer.entry
Canvas.sandbox_config -> renderer sandbox config
Canvas.bindings -> extension bindings / asset refs
Canvas runtime_bridge -> required runtime actions / permissions
```

## Runtime

优先复用 Canvas runtime preview 的加载与 bridge 模型。Canvas-derived extension 仍通过 Project extension installation 进入 session projection。

## Publish Flow

```text
Canvas -> extension package draft -> validate -> artifact -> Project install
```

首版可只支持同 Project 内 promote/install。
