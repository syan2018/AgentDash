import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Panel,
  useNodesState,
  useEdgesState,
  BackgroundVariant,
  MarkerType,
  type Node,
  type Edge,
  type Connection,
  type OnConnect,
  type NodeMouseHandler,
  ReactFlowProvider,
  useReactFlow,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import type {
  LifecycleEdge,
  LifecycleStepDefinition,
  InputPortDefinition,
  OutputPortDefinition,
  WorkflowDefinition,
} from "../../types";
import { useWorkflowStore } from "../../stores/workflowStore";
import {
  TARGET_KIND_LABEL,
} from "./shared-labels";
import { ValidationPanel } from "./ui/validation-panel";
import { DagNode, type DagNodeData } from "./ui/dag-node";
import { DagSidePanel } from "./ui/dag-side-panel";
import { DagLifecyclePanel } from "./ui/dag-lifecycle-panel";
import { applyDagreLayout, generateLinearEdges, wouldCreateCycle } from "./model/dag-layout";

// ─── 常量 ───

const NODE_TYPES = { dagNode: DagNode };

const POSITION_STORAGE_PREFIX = "agentdash:dag-positions:";

// ─── 位置持久化 ───

function loadPositions(lifecycleKey: string): Record<string, { x: number; y: number }> {
  try {
    const raw = localStorage.getItem(POSITION_STORAGE_PREFIX + lifecycleKey);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

function savePositions(lifecycleKey: string, positions: Record<string, { x: number; y: number }>) {
  try {
    localStorage.setItem(POSITION_STORAGE_PREFIX + lifecycleKey, JSON.stringify(positions));
  } catch {
    // localStorage 满了，忽略
  }
}

// ─── 数据转换：store ↔ ReactFlow ───

function stepsToNodes(
  steps: LifecycleStepDefinition[],
  entryStepKey: string,
  workflowDefs: WorkflowDefinition[],
  positions: Record<string, { x: number; y: number }>,
): Node<DagNodeData>[] {
  const wfMap = new Map(workflowDefs.map((d) => [d.key, d]));

  return steps.map((step, idx) => {
    const wf = step.workflow_key ? wfMap.get(step.workflow_key) : null;
    // port 归属在 step 级别，直接读取
    const data: DagNodeData = {
      stepKey: step.key,
      description: step.description,
      nodeType: step.node_type ?? "agent_node",
      workflowKey: step.workflow_key ?? null,
      workflowName: wf?.name ?? null,
      inputPorts: step.input_ports,
      outputPorts: step.output_ports,
      isEntryNode: step.key === entryStepKey,
    };

    return {
      id: step.key,
      type: "dagNode",
      position: positions[step.key] ?? { x: idx * 300, y: 0 },
      data,
    };
  });
}

function lifecycleEdgeId(e: LifecycleEdge): string {
  return e.kind === "flow"
    ? `flow:${e.from_node}->${e.to_node}`
    : `${e.from_node}:${e.from_port ?? ""}--${e.to_node}:${e.to_port ?? ""}`;
}

function lifecycleEdgesToRfEdges(edges: LifecycleEdge[]): Edge[] {
  return edges.map((e) => {
    const isFlow = e.kind === "flow";
    return {
      id: lifecycleEdgeId(e),
      source: e.from_node,
      sourceHandle: isFlow ? undefined : e.from_port ?? undefined,
      target: e.to_node,
      targetHandle: isFlow ? undefined : e.to_port ?? undefined,
      markerEnd: { type: MarkerType.ArrowClosed, width: 16, height: 16 },
      label: isFlow ? undefined : `${e.from_port ?? ""} → ${e.to_port ?? ""}`,
      labelStyle: { fontSize: 10, fill: "hsl(var(--muted-foreground))" },
      style: isFlow
        ? {
            stroke: "hsl(var(--primary))",
            strokeWidth: 2,
          }
        : {
            stroke: "hsl(var(--border))",
            strokeWidth: 2,
            strokeDasharray: "6 4",
          },
    };
  });
}

function rfEdgesToLifecycleEdges(rfEdges: Edge[]): LifecycleEdge[] {
  return rfEdges.map((e) => {
    if (e.sourceHandle && e.targetHandle) {
      return {
        kind: "artifact" as const,
        from_node: e.source,
        from_port: e.sourceHandle,
        to_node: e.target,
        to_port: e.targetHandle,
      };
    }
    return {
      kind: "flow" as const,
      from_node: e.source,
      to_node: e.target,
    };
  });
}

// ─── 内部编辑器（需要 ReactFlowProvider wrapper） ───

function LifecycleDagEditorInner() {
  const draft = useWorkflowStore((s) => s.lcEditor.draft);
  const originalId = useWorkflowStore((s) => s.lcEditor.originalId);
  const validation = useWorkflowStore((s) => s.lcEditor.validation);
  const isSaving = useWorkflowStore((s) => s.lcEditor.isSaving);
  const isValidating = useWorkflowStore((s) => s.lcEditor.isValidating);
  const isDirty = useWorkflowStore((s) => s.lcEditor.dirty);
  const error = useWorkflowStore((s) => s.lcEditor.error);
  const lifecycleDefinitions = useWorkflowStore((s) => s.lifecycleDefinitions);
  const workflowDefinitions = useWorkflowStore((s) => s.definitions);

  const updateLifecycleDraft = useWorkflowStore((s) => s.updateLifecycleDraft);
  const validateLifecycleDraft = useWorkflowStore((s) => s.validateLifecycleDraft);
  const saveLifecycleDraft = useWorkflowStore((s) => s.saveLifecycleDraft);
  const fetchDefinitions = useWorkflowStore((s) => s.fetchDefinitions);

  const reactFlowInstance = useReactFlow();

  // ── 选中节点 ──
  const [selectedNodeKey, setSelectedNodeKey] = useState<string | null>(null);

  // ── 加载关联的 workflow definitions ──
  const targetKind = draft?.target_kind;
  useEffect(() => {
    if (targetKind) void fetchDefinitions({ targetKind });
  }, [fetchDefinitions, targetKind]);

  // ── 可选择的 workflows ──
  const availableWorkflows = useMemo(
    () =>
      workflowDefinitions
        .filter((d) => d.target_kind === draft?.target_kind)
        .sort((a, b) => a.name.localeCompare(b.name, "zh-CN")),
    [workflowDefinitions, draft?.target_kind],
  );

  // ── 当前定义元数据 ──
  const currentDefinition = useMemo(
    () => (originalId ? lifecycleDefinitions.find((d) => d.id === originalId) ?? null : null),
    [lifecycleDefinitions, originalId],
  );

  // ── ReactFlow nodes & edges state ──
  const positions = useRef(loadPositions(draft?.key ?? "__new"));
  const [nodes, setNodes, onNodesChange] = useNodesState<Node<DagNodeData>>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // ── 从 draft 同步到 RF state（单一 effect，消除双 effect batching 竞态） ──
  const draftStepsRef = useRef<LifecycleStepDefinition[] | undefined>(undefined);
  const draftEdgesRef = useRef<LifecycleEdge[] | undefined>(undefined);
  useEffect(() => {
    if (!draft) return;

    const stepsChanged = draftStepsRef.current !== draft.steps;
    const edgesChanged = draftEdgesRef.current !== draft.edges;
    draftStepsRef.current = draft.steps;
    draftEdgesRef.current = draft.edges;

    if (stepsChanged || edgesChanged) {
      // steps 或 edges 引用变了 → 全量重建（含位置）
      setNodes(stepsToNodes(draft.steps, draft.entry_step_key, workflowDefinitions, positions.current));
      setEdges(lifecycleEdgesToRfEdges(draft.edges));
    } else {
      // 仅 workflowDefinitions / entry_step_key 变了 → 就地 patch node data
      const wfMap = new Map(workflowDefinitions.map((d) => [d.key, d]));
      const stepKeys = new Set(draft.steps.map((s) => s.key));
      setNodes((nds: Node<DagNodeData>[]) =>
        nds
          .filter((node: Node<DagNodeData>) => stepKeys.has(node.id))
          .map((node: Node<DagNodeData>) => {
            const step = draft.steps.find((s) => s.key === node.id)!;
            const wf = step.workflow_key ? wfMap.get(step.workflow_key) ?? null : null;
            return {
              ...node,
              data: {
                ...node.data,
                stepKey: step.key,
                description: step.description,
                nodeType: step.node_type ?? "agent_node",
                workflowKey: step.workflow_key ?? null,
                workflowName: wf?.name ?? null,
                outputPorts: step.output_ports,
                inputPorts: step.input_ports,
                isEntryNode: step.key === draft.entry_step_key,
              },
            };
          }),
      );
    }
  }, [draft, workflowDefinitions, setNodes, setEdges]);

  // ── 节点变更 → 过滤掉 remove（删除统一走 onDelete → store） ──
  const handleNodesChangeWrapped = useCallback(
    (changes: Parameters<typeof onNodesChange>[0]) => {
      // 不让 RF 自行删除节点；删除统一由 handleDelete 写 store → effect 重建
      const nonRemove = changes.filter((c) => c.type !== "remove");
      if (nonRemove.length > 0) onNodesChange(nonRemove);
      // 保存位置
      if (changes.some((c) => c.type === "position")) {
        requestAnimationFrame(() => {
          const pos: Record<string, { x: number; y: number }> = {};
          for (const n of reactFlowInstance.getNodes()) pos[n.id] = n.position;
          positions.current = pos;
          savePositions(draft?.key ?? "__new", pos);
        });
      }
    },
    [onNodesChange, reactFlowInstance, draft?.key],
  );

  // ── 边变更 → 过滤掉 remove（删除统一走 onDelete → store） ──
  const handleEdgesChangeWrapped = useCallback(
    (changes: Parameters<typeof onEdgesChange>[0]) => {
      const nonRemove = changes.filter((c) => c.type !== "remove");
      if (nonRemove.length > 0) onEdgesChange(nonRemove);
    },
    [onEdgesChange],
  );

  // ── Delete 键统一处理：节点 + 边删除都走 store ──
  const handleDelete = useCallback(
    ({ nodes: deletedNodes, edges: deletedEdges }: { nodes: Node[]; edges: Edge[] }) => {
      if (!draft) return;
      const deletedNodeKeys = new Set(deletedNodes.map((n) => n.id));

      let newSteps = draft.steps;
      let newEdges = draft.edges;

      if (deletedNodeKeys.size > 0) {
        newSteps = draft.steps.filter((s) => !deletedNodeKeys.has(s.key));
        newEdges = newEdges.filter(
          (e) => !deletedNodeKeys.has(e.from_node) && !deletedNodeKeys.has(e.to_node),
        );
        if (selectedNodeKey != null && deletedNodeKeys.has(selectedNodeKey)) {
          setSelectedNodeKey(null);
        }
      }

      if (deletedEdges.length > 0) {
        const deletedEdgeIds = new Set(deletedEdges.map((e) => e.id));
        newEdges = newEdges.filter((e) => !deletedEdgeIds.has(lifecycleEdgeId(e)));
      }

      updateLifecycleDraft({ steps: newSteps, edges: newEdges });
    },
    [draft, updateLifecycleDraft, selectedNodeKey],
  );

  // ── Edge 连接：校验 + 创建（只写 store，effect 重建 RF） ──
  const handleConnect: OnConnect = useCallback(
    (connection: Connection) => {
      if (!draft) return;
      if (!connection.source || !connection.target) return;
      if (connection.source === connection.target) return;
      // [Fix F] 用 draft.edges 做校验，不依赖 RF local state
      if (wouldCreateCycle(draft.edges, connection.source, connection.target)) return;

      // 判定 kind：source/target 都连到 port handle → artifact；否则 flow
      const hasSourcePort =
        !!connection.sourceHandle && connection.sourceHandle !== "__default_out";
      const hasTargetPort =
        !!connection.targetHandle && connection.targetHandle !== "__default_in";
      const isArtifactConnect = hasSourcePort || hasTargetPort;

      if (isArtifactConnect) {
        // Artifact edge：沿用原有 port 冲突校验 + 自动补 input port 逻辑
        if (
          connection.targetHandle &&
          draft.edges.some(
            (e) =>
              e.kind === "artifact" &&
              e.to_node === connection.target &&
              e.to_port === connection.targetHandle,
          )
        ) {
          return;
        }
        const fromPort = connection.sourceHandle ?? "__default_out";
        let toPort = connection.targetHandle ?? "__default_in";

        // ── 拖拽到 __default_in（节点 body）→ 自动创建同名 input port ──
        let newSteps = draft.steps;
        if (toPort === "__default_in" && fromPort !== "__default_out") {
          const targetStepIdx = draft.steps.findIndex((s) => s.key === connection.target);
          if (targetStepIdx >= 0) {
            const targetStep = draft.steps[targetStepIdx];
            const alreadyHas = targetStep.input_ports.some((p) => p.key === fromPort);
            if (!alreadyHas) {
              newSteps = draft.steps.map((s, i) =>
                i === targetStepIdx
                  ? { ...s, input_ports: [...s.input_ports, { key: fromPort, description: "", context_strategy: "full" as const }] }
                  : s,
              );
            }
            toPort = fromPort;
          }
        }

        const newEdge: LifecycleEdge = {
          kind: "artifact",
          from_node: connection.source,
          from_port: fromPort,
          to_node: connection.target,
          to_port: toPort,
        };
        updateLifecycleDraft({ steps: newSteps, edges: [...draft.edges, newEdge] });
      } else {
        // Flow edge：仅控制流，禁止重复（同一 from→to 只允许一条 flow edge）
        const existsFlow = draft.edges.some(
          (e) =>
            e.kind === "flow" &&
            e.from_node === connection.source &&
            e.to_node === connection.target,
        );
        if (existsFlow) return;

        const newEdge: LifecycleEdge = {
          kind: "flow",
          from_node: connection.source,
          to_node: connection.target,
        };
        updateLifecycleDraft({ edges: [...draft.edges, newEdge] });
      }
    },
    [draft, updateLifecycleDraft],
  );

  // ── 节点点击 → 选中 ──
  const handleNodeClick: NodeMouseHandler = useCallback((_event, node) => {
    setSelectedNodeKey(node.id);
  }, []);

  // ── 画布点击 → 取消选中 ──
  const handlePaneClick = useCallback(() => {
    setSelectedNodeKey(null);
  }, []);

  // ── 添加节点 ──
  const handleAddNode = useCallback(() => {
    if (!draft) return;
    const existingKeys = new Set(draft.steps.map((s) => s.key));
    let idx = draft.steps.length + 1;
    let key = `step_${idx}`;
    while (existingKeys.has(key)) {
      idx++;
      key = `step_${idx}`;
    }
    const newStep: LifecycleStepDefinition = {
      key,
      description: "",
      workflow_key: null,
      node_type: "agent_node",
      output_ports: [],
      input_ports: [],
    };
    updateLifecycleDraft({ steps: [...draft.steps, newStep] });
  }, [draft, updateLifecycleDraft]);

  // ── 删除节点 ──
  const handleRemoveNode = useCallback(
    (nodeKey: string) => {
      if (!draft) return;
      const newSteps = draft.steps.filter((s) => s.key !== nodeKey);
      const newEdges = draft.edges.filter(
        (e) => e.from_node !== nodeKey && e.to_node !== nodeKey,
      );
      updateLifecycleDraft({ steps: newSteps, edges: newEdges });
      if (selectedNodeKey === nodeKey) setSelectedNodeKey(null);
    },
    [draft, updateLifecycleDraft, selectedNodeKey],
  );

  // ── 更新节点（step 数据） ──
  const handleUpdateStep = useCallback(
    (nodeKey: string, patch: Partial<LifecycleStepDefinition>) => {
      if (!draft) return;
      const oldStep = draft.steps.find((s) => s.key === nodeKey);
      if (!oldStep) return;

      const newSteps = draft.steps.map((s) => (s.key === nodeKey ? { ...s, ...patch } : s));

      let newEdges = draft.edges;
      let newEntryKey = draft.entry_step_key;
      if (patch.key && patch.key !== nodeKey) {
        // 重命名：更新 edges 中的引用
        newEdges = draft.edges.map((e) => ({
          ...e,
          from_node: e.from_node === nodeKey ? patch.key! : e.from_node,
          to_node: e.to_node === nodeKey ? patch.key! : e.to_node,
        }));
        if (draft.entry_step_key === nodeKey) {
          newEntryKey = patch.key!;
        }
        setSelectedNodeKey(patch.key!);
        // 更新 localStorage position
        const pos = positions.current;
        if (pos[nodeKey]) {
          pos[patch.key!] = pos[nodeKey];
          delete pos[nodeKey];
          savePositions(draft.key, pos);
        }
      }

      updateLifecycleDraft({ steps: newSteps, edges: newEdges, entry_step_key: newEntryKey });
    },
    [draft, updateLifecycleDraft],
  );

  // ── 选中 step 元数据（供编辑回调复用） ──
  const updateLifecycleStep = useWorkflowStore((s) => s.updateLifecycleStep);
  const selectedStepIndex = useMemo(
    () => (selectedNodeKey != null ? draft?.steps.findIndex((s) => s.key === selectedNodeKey) ?? -1 : -1),
    [selectedNodeKey, draft?.steps],
  );

  // ── 导入 Workflow 推荐 Ports ──
  const handleImportRecommendedPorts = useCallback(() => {
    if (!draft || selectedStepIndex < 0) return;
    const step = draft.steps[selectedStepIndex];
    if (!step.workflow_key) return;
    const wf = workflowDefinitions.find((d) => d.key === step.workflow_key);
    if (!wf) return;
    const recOut = wf.contract.output_ports ?? [];
    const recIn = wf.contract.input_ports ?? [];
    // 合并：跳过已存在的同名 port
    const existingOutKeys = new Set(step.output_ports.map((p) => p.key));
    const existingInKeys = new Set(step.input_ports.map((p) => p.key));
    const newOut = [...step.output_ports, ...recOut.filter((p) => !existingOutKeys.has(p.key))];
    const newIn = [...step.input_ports, ...recIn.filter((p) => !existingInKeys.has(p.key))];
    updateLifecycleStep(selectedStepIndex, { output_ports: newOut, input_ports: newIn });
  }, [draft, selectedStepIndex, workflowDefinitions, updateLifecycleStep]);

  // ── 设为入口节点 ──
  const handleSetEntry = useCallback(
    (nodeKey: string) => {
      updateLifecycleDraft({ entry_step_key: nodeKey });
    },
    [updateLifecycleDraft],
  );

  // ── Auto-layout ──
  // [Fix C] 从 draft 构建节点，不读 RF state，避免残留幽灵节点参与布局
  const handleAutoLayout = useCallback(() => {
    if (!draft) return;
    const freshNodes = stepsToNodes(draft.steps, draft.entry_step_key, workflowDefinitions, positions.current);
    const freshEdges = lifecycleEdgesToRfEdges(draft.edges);
    const laid = applyDagreLayout(freshNodes, freshEdges);
    setNodes(laid);
    const pos: Record<string, { x: number; y: number }> = {};
    for (const n of laid) pos[n.id] = n.position;
    positions.current = pos;
    savePositions(draft.key ?? "__new", pos);
    requestAnimationFrame(() => reactFlowInstance.fitView({ padding: 0.2 }));
  }, [draft, workflowDefinitions, setNodes, reactFlowInstance]);

  // ── 自动线性连线 ──
  // [Fix B] 只写 store，不直接 setEdges
  const handleAutoWire = useCallback(() => {
    if (!draft) return;
    const nodeIds = draft.steps.map((s) => s.key);

    // port 归属在 step 级别，直接从 step.output_ports / input_ports 读取
    const getFirstOutputPort = (nodeId: string): string | null => {
      const step = draft.steps.find((s) => s.key === nodeId);
      if (!step) return null;
      if (step.output_ports.length > 0 && step.output_ports[0].key) return step.output_ports[0].key;
      return "__default_out";
    };
    const getFirstInputPort = (nodeId: string): string | null => {
      const step = draft.steps.find((s) => s.key === nodeId);
      if (!step) return null;
      if (step.input_ports.length > 0 && step.input_ports[0].key) return step.input_ports[0].key;
      return "__default_in";
    };

    const linearEdges = generateLinearEdges(nodeIds, getFirstOutputPort, getFirstInputPort);
    updateLifecycleDraft({ edges: rfEdgesToLifecycleEdges(linearEdges) });
  }, [draft, updateLifecycleDraft]);

  // ── 保存 ──
  const handleSave = useCallback(async () => {
    const result = await validateLifecycleDraft();
    if (result && result.issues.some((i) => i.severity === "error")) return;
    await saveLifecycleDraft();
  }, [validateLifecycleDraft, saveLifecycleDraft]);

  // ── Ctrl+S ──
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        if (!isSaving) void handleSave();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [handleSave, isSaving]);

  // ── 离开确认 ──
  useEffect(() => {
    if (!isDirty) return;
    const handler = (e: BeforeUnloadEvent) => { e.preventDefault(); };
    window.addEventListener("beforeunload", handler);
    return () => window.removeEventListener("beforeunload", handler);
  }, [isDirty]);

  // ── 首次挂载时自动 layout ──
  const hasAutoLayouted = useRef(false);
  useEffect(() => {
    if (hasAutoLayouted.current || nodes.length === 0) return;
    // 如果所有节点都在 (0,0) 附近，执行一次 auto-layout
    const allAtOrigin = nodes.every((n) => Math.abs(n.position.x) < 10 && Math.abs(n.position.y) < 10);
    if (allAtOrigin && nodes.length > 1) {
      hasAutoLayouted.current = true;
      requestAnimationFrame(handleAutoLayout);
    }
  }, [nodes, handleAutoLayout]);

  // ── 选中节点的 step 数据 ──
  const selectedStep = useMemo(
    () => (selectedNodeKey != null ? draft?.steps.find((s) => s.key === selectedNodeKey) ?? null : null),
    [selectedNodeKey, draft?.steps],
  );

  // ── Port 编辑回调（直接写 step 级 ports，通过 updateLifecycleStep 走 store） ──
  const handleOutputPortsChange = useCallback(
    (ports: OutputPortDefinition[]) => {
      if (selectedStepIndex < 0) return;
      updateLifecycleStep(selectedStepIndex, { output_ports: ports });
    },
    [selectedStepIndex, updateLifecycleStep],
  );

  const handleInputPortsChange = useCallback(
    (ports: InputPortDefinition[]) => {
      if (selectedStepIndex < 0) return;
      updateLifecycleStep(selectedStepIndex, { input_ports: ports });
    },
    [selectedStepIndex, updateLifecycleStep],
  );

  if (!draft) return null;

  const isNew = originalId === null;
  const hasErrors = validation?.issues.some((i) => i.severity === "error") ?? false;

  return (
    <div className="flex h-full">
      {/* 左侧：React Flow 画布 */}
      <div className="relative flex-1">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={handleNodesChangeWrapped}
          onEdgesChange={handleEdgesChangeWrapped}
          onConnect={handleConnect}
          onNodeClick={handleNodeClick}
          onPaneClick={handlePaneClick}
          onDelete={handleDelete}
          nodeTypes={NODE_TYPES}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          deleteKeyCode="Delete"
          proOptions={{ hideAttribution: true }}
        >
          <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="hsl(var(--border))" />
          <MiniMap
            nodeStrokeWidth={2}
            style={{ border: "1px solid hsl(var(--border))", borderRadius: 8 }}
          />
          <Controls
            showInteractive={false}
            style={{ border: "1px solid hsl(var(--border))", borderRadius: 8 }}
          />

          {/* 顶部工具栏 */}
          <Panel position="top-left">
            <div className="flex items-center gap-2 rounded-[10px] border border-border bg-background/95 px-3 py-2 shadow-sm backdrop-blur-sm">
              <button
                type="button"
                onClick={handleAddNode}
                className="agentdash-button-secondary px-2 py-1 text-xs"
              >
                + 添加节点
              </button>
              <button
                type="button"
                onClick={handleAutoLayout}
                className="agentdash-button-secondary px-2 py-1 text-xs"
              >
                自动布局
              </button>
              <button
                type="button"
                onClick={handleAutoWire}
                className="agentdash-button-secondary px-2 py-1 text-xs"
                title="按 steps 顺序自动生成线性连线"
              >
                线性连线
              </button>
              <div className="mx-1 h-5 w-px bg-border" />
              <button
                type="button"
                onClick={() => void validateLifecycleDraft()}
                disabled={isValidating}
                className="agentdash-button-secondary px-2 py-1 text-xs"
              >
                {isValidating ? "校验中…" : "校验"}
              </button>
              <button
                type="button"
                onClick={() => void handleSave()}
                disabled={isSaving || hasErrors}
                className="agentdash-button-primary px-3 py-1 text-xs"
              >
                {isSaving ? "保存中…" : "保存"}
              </button>
            </div>
          </Panel>

          {/* 右上状态 */}
          <Panel position="top-right">
            <div className="flex items-center gap-2">
              {isDirty && (
                <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-700">
                  未保存
                </span>
              )}
              {currentDefinition && (
                <span className="text-[10px] text-muted-foreground">
                  v{currentDefinition.version}
                </span>
              )}
            </div>
          </Panel>

          {/* 底部验证面板 */}
          {validation && validation.issues.length > 0 && (
            <Panel position="bottom-left">
              <div className="max-h-40 w-96 overflow-y-auto rounded-[10px] border border-border bg-background/95 shadow-sm backdrop-blur-sm">
                <ValidationPanel issues={validation.issues} />
              </div>
            </Panel>
          )}

          {/* 底部基本信息 */}
          <Panel position="bottom-right">
            <div className="rounded-[10px] border border-border bg-background/95 px-3 py-2 shadow-sm backdrop-blur-sm">
              <div className="flex items-center gap-3 text-[10px] text-muted-foreground">
                <span>Key: <span className="text-foreground">{draft.key || "—"}</span></span>
                <span>名称: <span className="text-foreground">{draft.name || "—"}</span></span>
                <span>类型: <span className="text-foreground">{TARGET_KIND_LABEL[draft.target_kind]}</span></span>
                <span>入口: <span className="text-foreground">{draft.entry_step_key || "—"}</span></span>
                <span>{draft.steps.length} 节点 · {draft.edges.length} 边</span>
              </div>
            </div>
          </Panel>
        </ReactFlow>

        {/* 错误横幅 */}
        {error && (
          <div className="absolute left-4 right-4 top-16 z-10 rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2">
            <p className="text-xs text-destructive">{error}</p>
          </div>
        )}
      </div>

      {/* 右侧面板：选中节点 → 节点配置 / 未选中 → Lifecycle 配置 */}
      <div className="w-80 shrink-0">
        {selectedStep ? (
          <DagSidePanel
            step={selectedStep}
            isEntry={selectedStep.key === draft.entry_step_key}
            availableWorkflows={availableWorkflows}
            outputPorts={selectedStep.output_ports}
            inputPorts={selectedStep.input_ports}
            onChange={(patch) => handleUpdateStep(selectedStep.key, patch)}
            onRemove={() => handleRemoveNode(selectedStep.key)}
            onClose={() => setSelectedNodeKey(null)}
            onSetEntry={() => handleSetEntry(selectedStep.key)}
            onOutputPortsChange={handleOutputPortsChange}
            onInputPortsChange={handleInputPortsChange}
            onImportRecommendedPorts={handleImportRecommendedPorts}
          />
        ) : (
          <DagLifecyclePanel
            lifecycleKey={draft.key}
            name={draft.name}
            description={draft.description}
            targetKind={draft.target_kind}
            entryStepKey={draft.entry_step_key}
            recommendedRoles={draft.recommended_roles}
            stepKeys={draft.steps.map((s) => s.key)}
            isNew={isNew}
            onChange={updateLifecycleDraft}
          />
        )}
      </div>
    </div>
  );
}

// ─── 导出（带 ReactFlowProvider wrapper） ───

export function LifecycleDagEditor() {
  return (
    <ReactFlowProvider>
      <LifecycleDagEditorInner />
    </ReactFlowProvider>
  );
}
