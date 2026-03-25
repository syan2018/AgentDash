import type { LifecycleStepDefinition } from "../../../types";
import { TRANSITION_POLICY_LABEL } from "../shared-labels";

export function StepSummary({ step }: { step: LifecycleStepDefinition }) {
  return (
    <div className="rounded-[10px] border border-border bg-background px-3 py-2 text-[11px]">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium text-foreground">{step.title}</span>
        <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
          {TRANSITION_POLICY_LABEL[step.transition.policy.kind]}
        </span>
        {step.session_binding !== "not_required" && (
          <span className="rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground">
            {step.session_binding === "required" ? "需要 Session" : "可挂接 Session"}
          </span>
        )}
      </div>
      <p className="mt-1 text-[10px] leading-5 text-muted-foreground">
        {step.primary_workflow_key}
        {step.transition.policy.next_step_key ? ` -> ${step.transition.policy.next_step_key}` : ""}
      </p>
      {step.description && (
        <p className="mt-1 text-[10px] leading-5 text-foreground/65">{step.description}</p>
      )}
    </div>
  );
}
