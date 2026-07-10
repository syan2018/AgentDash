import { useCallback, useEffect, useMemo, useState } from "react";

import {
  commitCanvas,
  createInteractionInstance,
  executeInteractionCommand,
  fetchCanvas,
  fetchInteractionInstance,
} from "../../services/canvas";
import {
  buildExactExtensionComponentAssetUrl,
  fetchProjectExtensionRuntime,
} from "../../services/extensionRuntime";
import type {
  CanvasDefinitionDto,
  InteractionInstanceViewDto,
} from "../../generated/interaction-contracts";
import type { ExtensionUiComponentProjectionResponse } from "../../generated/extension-runtime-contracts";
import type { JsonValue } from "../../generated/common-contracts";
import { ExtensionInteractionComponent } from "../extension-runtime";
import type { ExtensionUiComponentDescriptor } from "../extension-runtime";
import {
  fetchOperationWorkshopSurface,
  preflightWorkshopOperationScript,
  runWorkshopOperationScript,
} from "../../services/operationWorkshop";

export interface CanvasRuntimePanelProps {
  projectId: string | null;
  definitionId?: string | null;
  instanceId?: string | null;
  refreshRevision?: number;
  onOpenInteraction?(instanceId: string): void;
}

export function CanvasRuntimePanel({
  projectId,
  definitionId = null,
  instanceId = null,
  refreshRevision = 0,
  onOpenInteraction,
}: CanvasRuntimePanelProps) {
  const [definition, setDefinition] = useState<CanvasDefinitionDto | null>(null);
  const [instance, setInstance] = useState<InteractionInstanceViewDto | null>(null);
  const [components, setComponents] = useState<ExtensionUiComponentProjectionResponse[]>([]);
  const [entrySource, setEntrySource] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scriptResult, setScriptResult] = useState<unknown>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const nextInstance = instanceId ? await fetchInteractionInstance(instanceId) : null;
      const nextDefinitionId = definitionId ?? nextInstance?.instance.definition_id ?? null;
      const nextDefinition = nextDefinitionId ? await fetchCanvas(nextDefinitionId) : null;
      setInstance(nextInstance);
      setDefinition(nextDefinition);
      setEntrySource(
        nextDefinition?.source_bundle.files.find(
          (file) => file.path === nextDefinition.source_bundle.entry_file,
        )?.content ?? "",
      );
      if (projectId && nextDefinition?.component_bindings.length) {
        const runtime = await fetchProjectExtensionRuntime(projectId);
        setComponents(runtime.ui_components);
      } else {
        setComponents([]);
      }
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "Canvas 加载失败");
    }
  }, [definitionId, instanceId, projectId]);

  useEffect(() => {
    queueMicrotask(() => {
      void load();
    });
  }, [load, refreshRevision]);

  const handleSave = useCallback(async () => {
    if (!definition || !definition.access.can_edit_source) return;
    const entry = definition.source_bundle.files.find(
      (file) => file.path === definition.source_bundle.entry_file,
    );
    if (!entry || entry.content === entrySource) return;
    setBusy(true);
    setError(null);
    try {
      const updated = await commitCanvas(definition.definition_id, {
        base_revision_id: definition.current_revision_id,
        changeset: {
          file_changes: [{ kind: "upsert", file: { ...entry, content: entrySource } }],
        },
      });
      setDefinition(updated);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Canvas 保存失败");
    } finally {
      setBusy(false);
    }
  }, [definition, entrySource]);

  const handleCreateInstance = useCallback(async () => {
    if (!definition) return;
    setBusy(true);
    setError(null);
    try {
      const created = await createInteractionInstance(definition.definition_id, {
        definition_revision_id: definition.current_revision_id,
      });
      setInstance(created);
      onOpenInteraction?.(created.instance.instance_id);
    } catch (createError) {
      setError(createError instanceof Error ? createError.message : "Interaction 创建失败");
    } finally {
      setBusy(false);
    }
  }, [definition, onOpenInteraction]);

  const rhaiSource = definition?.source_bundle.files.find((file) => file.path.endsWith(".rhai")) ?? null;
  const handleRunScript = useCallback(async () => {
    if (!projectId || !definition || !rhaiSource) return;
    setBusy(true);
    setError(null);
    setScriptResult(null);
    try {
      const context = instance
        ? { kind: "interaction" as const, instance_id: instance.instance.instance_id }
        : { kind: "canvas" as const, definition_id: definition.definition_id };
      const surface = await fetchOperationWorkshopSurface(projectId, { context });
      const program = {
        language: "rhai_v1",
        host_api_version: 1,
        source: rhaiSource.content,
        input: instance?.instance.state ?? {},
        requested_operations: surface.operations.filter((operation) => operation.ready).map((operation) => operation.operation_ref),
        limits: {
          timeout_ms: 30_000,
          max_source_bytes: 262_144,
          max_input_bytes: 1_048_576,
          max_output_bytes: 1_048_576,
          max_rhai_operations: 100_000,
          max_call_levels: 32,
          max_string_size: 1_048_576,
          max_array_size: 1_000,
          max_map_size: 500,
          max_operation_calls: 32,
          max_parallel_operations: 4,
        },
      };
      const preflight = await preflightWorkshopOperationScript(projectId, { context, program });
      const output = await runWorkshopOperationScript(projectId, { context, program, token: preflight.token });
      setScriptResult(output.outcome);
    } catch (scriptError) {
      setError(scriptError instanceof Error ? scriptError.message : "OperationScript 执行失败");
    } finally {
      setBusy(false);
    }
  }, [definition, instance, projectId, rhaiSource]);

  const preview = useMemo(() => {
    if (!entrySource) return null;
    return (
      <iframe
        title="Canvas definition preview"
        srcDoc={entrySource}
        sandbox="allow-scripts"
        className="h-full min-h-[320px] w-full rounded-[8px] border border-border bg-white"
      />
    );
  }, [entrySource]);

  if (!definition) {
    return <div className="p-4 text-sm text-muted-foreground">{error ?? "正在加载 Canvas…"}</div>;
  }

  return (
    <div className="flex h-full min-h-[420px] flex-col gap-3 p-3">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-foreground">{definition.title}</h3>
          <p className="text-xs text-muted-foreground">
            revision {definition.revision_number} · canvas://{definition.definition_id}
            {instance ? ` · interaction://${instance.instance.instance_id}` : ""}
          </p>
        </div>
        <div className="flex gap-2">
          {definition.access.can_edit_source && (
            <button className="agentdash-button-secondary" disabled={busy} onClick={() => void handleSave()}>
              保存 revision
            </button>
          )}
          {!instance && (
            <button className="agentdash-button-primary" disabled={busy} onClick={() => void handleCreateInstance()}>
              启动 Interaction
            </button>
          )}
          {rhaiSource && (
            <button className="agentdash-button-secondary" disabled={busy} onClick={() => void handleRunScript()}>
              运行 {rhaiSource.path}
            </button>
          )}
        </div>
      </header>

      {error && <div className="rounded-[8px] border border-destructive/40 bg-destructive/10 p-2 text-xs text-destructive">{error}</div>}

      <div className="grid min-h-0 flex-1 gap-3 lg:grid-cols-[minmax(280px,0.8fr)_minmax(360px,1.2fr)]">
        <section className="flex min-h-[320px] flex-col gap-2">
          <p className="text-xs font-medium text-muted-foreground">{definition.source_bundle.entry_file}</p>
          <textarea
            value={entrySource}
            onChange={(event) => setEntrySource(event.target.value)}
            readOnly={!definition.access.can_edit_source}
            className="min-h-[320px] flex-1 resize-none rounded-[8px] border border-border bg-background p-3 font-mono text-xs text-foreground outline-none"
          />
        </section>
        <section className="min-h-[320px]">{preview}</section>
      </div>

      {instance && (
        <section className="space-y-3 rounded-[8px] border border-border p-3">
          <div>
            <p className="text-xs font-medium text-foreground">Canonical state</p>
            <pre className="mt-2 max-h-48 overflow-auto rounded-[8px] bg-secondary/30 p-3 text-xs">{JSON.stringify(instance.instance.state, null, 2)}</pre>
          </div>
          {definition.component_bindings.map((binding) => (
            <BoundComponent
              key={binding.binding_key}
              projectId={projectId}
              instance={instance}
              binding={binding}
              components={components}
              onInstanceChange={setInstance}
            />
          ))}
        </section>
      )}
      {scriptResult !== null && (
        <pre className="max-h-52 overflow-auto rounded-[8px] border border-border bg-secondary/30 p-3 text-xs">
          {JSON.stringify(scriptResult, null, 2)}
        </pre>
      )}
    </div>
  );
}

interface BoundComponentProps {
  projectId: string | null;
  instance: InteractionInstanceViewDto;
  binding: CanvasDefinitionDto["component_bindings"][number];
  components: ExtensionUiComponentProjectionResponse[];
  onInstanceChange(value: InteractionInstanceViewDto): void;
}

function BoundComponent({ projectId, instance, binding, components, onInstanceChange }: BoundComponentProps) {
  const component = components.find((item) => item.component_key === binding.component_ref);
  const runtimeBinding = instance.runtime_bindings.find((item) => item.slot_key === `component:${binding.binding_key}`);
  if (!projectId || !component || !component.available || component.renderer.kind !== "iframe"
    || component.contract_version !== 1 || component.sandbox_profile !== "isolated_v1"
    || runtimeBinding?.target.kind !== "artifact") {
    return <div className="text-xs text-muted-foreground">Component unavailable: {binding.component_ref}</div>;
  }
  const artifactSrc = buildExactExtensionComponentAssetUrl(
    projectId,
    runtimeBinding.target.artifact_ref,
    runtimeBinding.target.digest,
    component.component_key,
    component.renderer.entry,
  );
  const descriptor = component as unknown as ExtensionUiComponentDescriptor;
  return (
    <ExtensionInteractionComponent
      descriptor={descriptor}
      artifactSrc={artifactSrc}
      componentInstanceId={`${instance.instance.instance_id}:${binding.binding_key}`}
      props={binding.props}
      stateProjection={instance.instance.state}
      theme="light"
      locale="zh-CN"
      onEvent={async (eventType, payload) => {
        const eventBinding = binding.event_commands.find((item) => item.event_type === eventType);
        if (!eventBinding) throw new Error(`未绑定 component event: ${eventType}`);
        const result = await executeInteractionCommand(instance.instance.instance_id, {
          command_id: crypto.randomUUID(),
          command_key: eventBinding.command_key,
          payload: payload as JsonValue,
          expected_state_revision: instance.instance.state_revision,
        });
        onInstanceChange({ ...instance, instance: result.instance });
        return { state_revision: result.instance.state_revision, duplicate: result.duplicate };
      }}
    />
  );
}

export default CanvasRuntimePanel;
