/**
 * LifecycleDagCanvas —— 纯 DAG 画布（ReactFlow）的可复用封装。
 *
 * 受控组件：
 * - 输入：steps / edges / entry_step_key + 可选 workflowDefs（供节点 label 渲染）
 * - 输出：onStepsChange / onEdgesChange（整体替换），以及 onSelectStep
 * - 不读写 store；所有副作用（位置持久化、布局计算）仍由画布自身管理
 */

import { useCallback, useEffect, useMemo, useRef } from "react";
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
  WorkflowDefinition,
} from "../../../types";
import { DagNode, type DagNodeData } from "./dag-node";
import { applyDagreLayout, generateLinearEdges, wouldCreateCycle } from "../model/dag-layout";
import { syncLifecycleStepPortsForArtifactEdges } from "../model/lifecycle-port-sync";

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
    // 忽略 quota 满
  }
}

// ─── 数据转换：domain ↔ ReactFlow ───

function stepsToNodes(
  steps: LifecycleStepDefinition[],
  entryStepKey: string,
  workflowDefs: WorkflowDefinition[],
  positions: Record<string, { x: number; y: number }>,
): Node<DagNodeData>[] {
  const wfMap = new Map(workflowDefs.map((d) => [d.key, d]));
  return steps.map((step, idx) => {
    const wf = step.workflow_key ? wfMap.get(step.workflow_key) : null;
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
        ? { stroke: "hsl(var(--primary))", strokeWidth: 2 }
        : { stroke: "hsl(var(--border))", strokeWidth: 2, strokeDasharray: "6 4" },
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
    return { kind: "flow" as const, from_node: e.source, to_node: e.target };
  });
}

// ─── Props ───

export interface LifecycleDagCanvasProps {
  /** 用作 position localStorage 桶的 key；建议用 lifecycle_key，新建用 "__new__"。 */
  storageKey: string;
  steps: LifecycleStepDefinition[];
  edges: LifecycleEdge[];
  entryStepKey: string;
  workflowDefs: WorkflowDefinition[];
  selectedStepKey: string | null;
  onSelectStep: (stepKey: string | null) => void;
  onStepsChange: (next: LifecycleStepDefinition[]) => void;
  onEdgesChange: (next: LifecycleEdge[]) => void;
  /** 顶部工具栏插槽（校验/保存按钮等）。 */
  toolbarExtras?: React.ReactNode;
  /** 右上状态插槽。 */
  statusExtras?: React.ReactNode;
  /** 底部叠加面板（例如 ValidationPanel）。 */
  bottomLeftOverlay?: React.ReactNode;
  /** 新增 step 触发：由外层完成 key 生成和 Form→DAG 升级语义。 */
  onAddStep?: () => void;
}

// ─── 内部 inner（需要在 ReactFlowProvider 内） ───

function LifecycleDagCanvasInner({
  storageKey,
  steps,
  edges,
  entryStepKey,
  workflowDefs,
  selectedStepKey,
  onSelectStep,
  onStepsChange,
  onEdgesChange,
  toolbarExtras,
  statusExtras,
  bottomLeftOverlay,
  onAddStep,
}: LifecycleDagCanvasProps) {
  const reactFlowInstance = useReactFlow();
  const positions = useRef(loadPositions(storageKey));

  const [nodes, setNodes, onNodesChange] = useNodesState<Node<DagNodeData>>([]);
  const [rfEdges, setRfEdges, onRfEdgesChange] = useEdgesState<Edge>([]);

  // ── 从 props 同步到 RF state ──
  const lastStepsRef = useRef<LifecycleStepDefinition[] | undefined>(undefined);
  const lastEdgesRef = useRef<LifecycleEdge[] | undefined>(undefined);
  useEffect(() => {
    const stepsChanged = lastStepsRef.current !== steps;
    const edgesChanged = lastEdgesRef.current !== edges;
    lastStepsRef.current = steps;
    lastEdgesRef.current = edges;

    if (stepsChanged || edgesChanged) {
      setNodes(stepsToNodes(steps, entryStepKey, workflowDefs, positions.current));
      setRfEdges(lifecycleEdgesToRfEdges(edges));
    } else {
      // 仅 workflowDefs / entry_step_key 变化 → 就地 patch
      const wfMap = new Map(workflowDefs.map((d) => [d.key, d]));
      const stepKeys = new Set(steps.map((s) => s.key));
      setNodes((nds: Node<DagNodeData>[]) =>
        nds
          .filter((node) => stepKeys.has(node.id))
          .map((node) => {
            const step = steps.find((s) => s.key === node.id)!;
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
                isEntryNode: step.key === entryStepKey,
              },
            };
          }),
      );
    }
  }, [steps, edges, entryStepKey, workflowDefs, setNodes, setRfEdges]);

  const handleNodesChangeWrapped = useCallback(
    (changes: Parameters<typeof onNodesChange>[0]) => {
      const nonRemove = changes.filter((c) => c.type !== "remove");
      if (nonRemove.length > 0) onNodesChange(nonRemove);
      if (changes.some((c) => c.type === "position")) {
        requestAnimationFrame(() => {
          const pos: Record<string, { x: number; y: number }> = {};
          for (const n of reactFlowInstance.getNodes()) pos[n.id] = n.position;
          positions.current = pos;
          savePositions(storageKey, pos);
        });
      }
    },
    [onNodesChange, reactFlowInstance, storageKey],
  );

  const handleEdgesChangeWrapped = useCallback(
    (changes: Parameters<typeof onRfEdgesChange>[0]) => {
      const nonRemove = changes.filter((c) => c.type !== "remove");
      if (nonRemove.length > 0) onRfEdgesChange(nonRemove);
    },
    [onRfEdgesChange],
  );

  // ── Delete 键：统一回写 props ──
  const handleDelete = useCallback(
    ({ nodes: deletedNodes, edges: deletedEdges }: { nodes: Node[]; edges: Edge[] }) => {
      const deletedNodeKeys = new Set(deletedNodes.map((n) => n.id));
      let newSteps = steps;
      let newEdges = edges;

      if (deletedNodeKeys.size > 0) {
        newSteps = steps.filter((s) => !deletedNodeKeys.has(s.key));
        newEdges = newEdges.filter(
          (e) => !deletedNodeKeys.has(e.from_node) && !deletedNodeKeys.has(e.to_node),
        );
        if (selectedStepKey != null && deletedNodeKeys.has(selectedStepKey)) {
          onSelectStep(null);
        }
      }

      if (deletedEdges.length > 0) {
        const deletedEdgeIds = new Set(deletedEdges.map((e) => e.id));
        newEdges = newEdges.filter((e) => !deletedEdgeIds.has(lifecycleEdgeId(e)));
      }

      if (newSteps !== steps) onStepsChange(newSteps);
      if (newEdges !== edges) onEdgesChange(newEdges);
    },
    [steps, edges, selectedStepKey, onSelectStep, onStepsChange, onEdgesChange],
  );

  // ── Connect：新边 ──
  const handleConnect: OnConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target) return;
      if (connection.source === connection.target) return;
      if (wouldCreateCycle(edges, connection.source, connection.target)) return;

      const hasSourcePort = !!connection.sourceHandle && connection.sourceHandle !== "__default_out";
      const hasTargetPort = !!connection.targetHandle && connection.targetHandle !== "__default_in";
      const isArtifactConnect = hasSourcePort || hasTargetPort;

      if (isArtifactConnect) {
        if (
          connection.targetHandle &&
          edges.some(
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

        let newSteps = steps;
        if (toPort === "__default_in" && fromPort !== "__default_out") {
          const targetIdx = steps.findIndex((s) => s.key === connection.target);
          if (targetIdx >= 0) {
            const targetStep = steps[targetIdx];
            const has = targetStep.input_ports.some((p) => p.key === fromPort);
            if (!has) {
              newSteps = steps.map((s, i) =>
                i === targetIdx
                  ? {
                      ...s,
                      input_ports: [
                        ...s.input_ports,
                        { key: fromPort, description: "", context_strategy: "full" as const },
                      ],
                    }
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
        const nextEdges = [...edges, newEdge];
        const synced = syncLifecycleStepPortsForArtifactEdges({
          steps: newSteps,
          edges: nextEdges,
          workflows: workflowDefs,
        });
        if (synced.changed || newSteps !== steps) onStepsChange(synced.steps);
        onEdgesChange(nextEdges);
      } else {
        const existsFlow = edges.some(
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
        onEdgesChange([...edges, newEdge]);
      }
    },
    [edges, steps, workflowDefs, onStepsChange, onEdgesChange],
  );

  const handleNodeClick: NodeMouseHandler = useCallback(
    (_e, node) => {
      onSelectStep(node.id);
    },
    [onSelectStep],
  );

  const handlePaneClick = useCallback(() => {
    onSelectStep(null);
  }, [onSelectStep]);

  const handleAutoLayout = useCallback(() => {
    const freshNodes = stepsToNodes(steps, entryStepKey, workflowDefs, positions.current);
    const freshEdges = lifecycleEdgesToRfEdges(edges);
    const laid = applyDagreLayout(freshNodes, freshEdges);
    setNodes(laid);
    const pos: Record<string, { x: number; y: number }> = {};
    for (const n of laid) pos[n.id] = n.position;
    positions.current = pos;
    savePositions(storageKey, pos);
    requestAnimationFrame(() => reactFlowInstance.fitView({ padding: 0.2 }));
  }, [steps, edges, entryStepKey, workflowDefs, setNodes, reactFlowInstance, storageKey]);

  const handleAutoWire = useCallback(() => {
    const nodeIds = steps.map((s) => s.key);
    const getFirstOutputPort = (nodeId: string): string | null => {
      const step = steps.find((s) => s.key === nodeId);
      if (!step) return null;
      if (step.output_ports.length > 0 && step.output_ports[0].key) return step.output_ports[0].key;
      return "__default_out";
    };
    const getFirstInputPort = (nodeId: string): string | null => {
      const step = steps.find((s) => s.key === nodeId);
      if (!step) return null;
      if (step.input_ports.length > 0 && step.input_ports[0].key) return step.input_ports[0].key;
      return "__default_in";
    };
    const linearEdges = generateLinearEdges(nodeIds, getFirstOutputPort, getFirstInputPort);
    onEdgesChange(rfEdgesToLifecycleEdges(linearEdges));
  }, [steps, onEdgesChange]);

  // ── 首次挂载自动 layout（所有节点 ≈ (0,0)）──
  const hasAutoLayouted = useRef(false);
  useEffect(() => {
    if (hasAutoLayouted.current || nodes.length === 0) return;
    const allAtOrigin = nodes.every(
      (n) => Math.abs(n.position.x) < 10 && Math.abs(n.position.y) < 10,
    );
    if (allAtOrigin && nodes.length > 1) {
      hasAutoLayouted.current = true;
      requestAnimationFrame(handleAutoLayout);
    }
  }, [nodes, handleAutoLayout]);

  const selectedIds = useMemo(
    () => (selectedStepKey ? new Set([selectedStepKey]) : new Set<string>()),
    [selectedStepKey],
  );
  const nodesForRender = useMemo(
    () =>
      nodes.map((n) =>
        selectedIds.has(n.id) ? { ...n, selected: true } : n.selected ? { ...n, selected: false } : n,
      ),
    [nodes, selectedIds],
  );

  return (
    <ReactFlow
      nodes={nodesForRender}
      edges={rfEdges}
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

      <Panel position="top-left">
        <div className="flex items-center gap-2 rounded-[10px] border border-border bg-background/95 px-3 py-2 shadow-sm backdrop-blur-sm">
          {onAddStep && (
            <button
              type="button"
              onClick={onAddStep}
              className="agentdash-button-secondary px-2 py-1 text-xs"
            >
              + 添加节点
            </button>
          )}
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
          {toolbarExtras && (
            <>
              <div className="mx-1 h-5 w-px bg-border" />
              {toolbarExtras}
            </>
          )}
        </div>
      </Panel>

      {statusExtras && <Panel position="top-right">{statusExtras}</Panel>}
      {bottomLeftOverlay && <Panel position="bottom-left">{bottomLeftOverlay}</Panel>}
    </ReactFlow>
  );
}

// ─── 导出封装 ───

export function LifecycleDagCanvas(props: LifecycleDagCanvasProps) {
  return (
    <ReactFlowProvider>
      <LifecycleDagCanvasInner {...props} />
    </ReactFlowProvider>
  );
}
