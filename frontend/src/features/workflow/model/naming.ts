/**
 * Workflow/Lifecycle 编辑器里的命名规则。
 *
 * key 统一使用小写 snake_case，显示名保留用户可读文本。
 */

import type { WorkflowDefinition } from "../../../types";

export function normalizeIdentifier(value: string, fallback: string): string {
  const normalized = value
    .trim()
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_+/g, "_");

  return normalized || fallback;
}

export function uniqueIdentifier(value: string, usedKeys: Iterable<string>, fallback: string): string {
  const used = new Set(usedKeys);
  const base = normalizeIdentifier(value, fallback);
  let candidate = base;
  let index = 2;

  while (used.has(candidate)) {
    candidate = `${base}_${index}`;
    index += 1;
  }

  return candidate;
}

export function formatDisplaySegment(value: string, fallback: string): string {
  const words = value
    .trim()
    .replace(/[_-]+/g, " ")
    .split(/\s+/)
    .filter(Boolean);

  if (words.length === 0) {
    return fallback;
  }

  return words
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

export function buildLifecycleStepWorkflowNames(input: {
  lifecycleKey: string;
  lifecycleDisplayName: string;
  stepKey: string;
  stepDisplayName?: string;
  existingWorkflows: WorkflowDefinition[];
}): { key: string; name: string } {
  const lifecycleKey = normalizeIdentifier(input.lifecycleKey, "lifecycle");
  const stepKey = normalizeIdentifier(input.stepKey, "step");
  const key = uniqueIdentifier(
    `${lifecycleKey}_${stepKey}`,
    input.existingWorkflows.map((definition) => definition.key),
    "workflow",
  );
  const displayLifecycle = input.lifecycleDisplayName.trim() || input.lifecycleKey.trim() || "Lifecycle";
  const displayStep = formatDisplaySegment(input.stepDisplayName ?? input.stepKey, "Step");

  return {
    key,
    name: `${displayLifecycle} / ${displayStep}`,
  };
}
