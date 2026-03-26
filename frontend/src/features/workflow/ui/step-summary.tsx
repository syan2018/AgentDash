import type { LifecycleStepDefinition } from "../../../types";

export function StepSummary({
  step,
  workflowName,
}: {
  step: LifecycleStepDefinition;
  /** Resolved workflow definition name when step.workflow_key is set */
  workflowName?: string | null;
}) {
  const bound = Boolean(step.workflow_key?.trim());
  const workflowLine = bound
    ? (workflowName?.trim() ? `Workflow: ${workflowName}` : `Workflow: ${step.workflow_key}`)
    : "Manual Step";

  return (
    <div className="rounded-[10px] border border-border bg-background px-3 py-2 text-[11px]">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-foreground">{step.key || "(no key)"}</span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
          {workflowLine}
        </span>
      </div>
      {step.description && (
        <p className="mt-1 text-[10px] leading-5 text-foreground/65">{step.description}</p>
      )}
    </div>
  );
}
