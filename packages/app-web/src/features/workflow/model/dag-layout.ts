import dagre from "@dagrejs/dagre";
import type { Node, Edge } from "@xyflow/react";

import type { ActivityDefinition, ActivityTransition } from "../../../types";

const NODE_WIDTH = 240;
const NODE_HEIGHT = 100;
const RANK_SEP = 100;
const NODE_SEP = 50;

/**
 * 使用 dagre 算法对 DAG 节点进行自动布局（从左到右）。
 * 返回带有新 position 的节点数组（不修改原数组）。
 */
export function applyDagreLayout<N extends Node = Node>(nodes: N[], edges: Edge[]): N[] {
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: "LR", ranksep: RANK_SEP, nodesep: NODE_SEP });

  for (const node of nodes) {
    g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }
  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }

  dagre.layout(g);

  return nodes.map((node) => {
    const pos = g.node(node.id);
    return {
      ...node,
      position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 },
    };
  });
}

/**
 * 按 steps 数组顺序生成线性 edges（前一个 node 的第一个 output port → 后一个 node 的第一个 input port）。
 * 如果某个 node 没有对应的 port，则使用 default handle 连接。
 */
export function generateLinearEdges(
  nodeIds: string[],
  getOutputPort: (nodeId: string) => string | null,
  getInputPort: (nodeId: string) => string | null,
): Edge[] {
  const edges: Edge[] = [];
  for (let i = 0; i < nodeIds.length - 1; i++) {
    const src = nodeIds[i];
    const tgt = nodeIds[i + 1];
    const srcPort = getOutputPort(src);
    const tgtPort = getInputPort(tgt);
    if (srcPort && tgtPort) {
      edges.push({
        id: `${src}:${srcPort}--${tgt}:${tgtPort}`,
        source: src,
        sourceHandle: srcPort,
        target: tgt,
        targetHandle: tgtPort,
      });
    }
  }
  return edges;
}

/**
 * 检测 lifecycle 内是否存在"未设阈值的环"——环上的所有边 max_traversals=null
 * 且环上的所有 activity iteration_policy.max_attempts=null，运行时无任何收敛机制。
 * 返回受影响的 SCC（活动 key 列表 + 内部 transition 索引），供 validator 转 warning。
 */
export function findUnboundedCycles(input: {
  activities: ActivityDefinition[];
  transitions: ActivityTransition[];
}): Array<{ activityKeys: string[]; transitionIndices: number[] }> {
  const { activities, transitions } = input;
  const adj = new Map<string, string[]>();
  for (const a of activities) adj.set(a.key, []);
  for (const t of transitions) {
    if (adj.has(t.from)) adj.get(t.from)!.push(t.to);
  }

  // Tarjan SCC（迭代式以避免深图爆栈）
  let nextIndex = 0;
  const indices = new Map<string, number>();
  const lowlinks = new Map<string, number>();
  const onStack = new Set<string>();
  const sccStack: string[] = [];
  const sccs: string[][] = [];

  type Frame = { v: string; iter: Iterator<string> };
  for (const start of activities.map((a) => a.key)) {
    if (indices.has(start)) continue;
    const work: Frame[] = [];
    const pushNode = (v: string) => {
      indices.set(v, nextIndex);
      lowlinks.set(v, nextIndex);
      nextIndex += 1;
      sccStack.push(v);
      onStack.add(v);
      work.push({ v, iter: (adj.get(v) ?? [])[Symbol.iterator]() });
    };
    pushNode(start);
    while (work.length > 0) {
      const top = work[work.length - 1];
      const next = top.iter.next();
      if (next.done) {
        if (lowlinks.get(top.v) === indices.get(top.v)) {
          const scc: string[] = [];
          while (sccStack.length > 0) {
            const w = sccStack.pop()!;
            onStack.delete(w);
            scc.push(w);
            if (w === top.v) break;
          }
          sccs.push(scc);
        }
        work.pop();
        if (work.length > 0) {
          const parent = work[work.length - 1];
          lowlinks.set(parent.v, Math.min(lowlinks.get(parent.v)!, lowlinks.get(top.v)!));
        }
        continue;
      }
      const w = next.value;
      if (!indices.has(w)) {
        pushNode(w);
      } else if (onStack.has(w)) {
        lowlinks.set(top.v, Math.min(lowlinks.get(top.v)!, indices.get(w)!));
      }
    }
  }

  const result: Array<{ activityKeys: string[]; transitionIndices: number[] }> = [];
  for (const scc of sccs) {
    const set = new Set(scc);
    const internal: number[] = [];
    let hasSelfLoop = false;
    transitions.forEach((t, idx) => {
      if (set.has(t.from) && set.has(t.to)) {
        internal.push(idx);
        if (t.from === t.to) hasSelfLoop = true;
      }
    });
    if (scc.length === 1 && !hasSelfLoop) continue;
    if (internal.length === 0) continue;
    const traversalBound = internal.some((idx) => transitions[idx].max_traversals != null);
    const attemptsBound = activities
      .filter((a) => set.has(a.key))
      .some((a) => a.iteration_policy.max_attempts != null);
    if (!traversalBound && !attemptsBound) {
      result.push({ activityKeys: scc, transitionIndices: internal });
    }
  }
  return result;
}

/**
 * 检测添加一条 source→target 边是否会产生环。
 * 接受 ReactFlow edge、旧 step edge、Activity transition 三种边形态。
 */
export function wouldCreateCycle(
  existingEdges: ReadonlyArray<{ source?: string; target?: string; from_node?: string; to_node?: string; from?: string; to?: string }>,
  newSource: string,
  newTarget: string,
): boolean {
  const adj = new Map<string, string[]>();
  for (const edge of existingEdges) {
    const s = edge.source ?? edge.from_node ?? edge.from ?? "";
    const t = edge.target ?? edge.to_node ?? edge.to ?? "";
    const list = adj.get(s) ?? [];
    list.push(t);
    adj.set(s, list);
  }
  const visited = new Set<string>();
  const stack = [newTarget];
  while (stack.length > 0) {
    const cur = stack.pop();
    if (cur == null) continue;
    if (cur === newSource) return true;
    if (visited.has(cur)) continue;
    visited.add(cur);
    for (const next of adj.get(cur) ?? []) {
      stack.push(next);
    }
  }
  return false;
}
