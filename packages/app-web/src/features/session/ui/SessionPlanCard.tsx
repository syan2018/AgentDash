/**
 * 计划卡片
 *
 * 显示 Agent 的执行计划（TurnPlanStep[]）
 */

import { memo, useState } from "react";
import type { TurnPlanStep, TurnPlanStepStatus } from "../../../generated/backbone-protocol";

export interface AcpPlanCardProps {
  steps: TurnPlanStep[];
  collapsible?: boolean;
  defaultCollapsed?: boolean;
}

export const AcpPlanCard = memo(function AcpPlanCard({
  steps,
  collapsible = true,
  defaultCollapsed = false,
}: AcpPlanCardProps) {
  const [isCollapsed, setIsCollapsed] = useState(defaultCollapsed);

  if (steps.length === 0) {
    return null;
  }

  const completedCount = steps.filter((s) => s.status === "completed").length;
  const inProgressCount = steps.filter((s) => s.status === "inProgress").length;
  const progress = Math.round((completedCount / steps.length) * 100);

  const cardContent = (
    <>
      <div className="mb-3 h-1.5 w-full overflow-hidden rounded-full bg-secondary">
        <div
          className="h-full rounded-full bg-primary transition-all duration-300"
          style={{ width: `${progress}%` }}
        />
      </div>

      <ul className="space-y-2">
        {steps.map((step, index) => (
          <PlanStepItem key={index} step={step} index={index} />
        ))}
      </ul>

      <div className="mt-3 flex items-center gap-4 border-t border-border pt-3 text-xs text-muted-foreground">
        <span>总计: {steps.length}</span>
        <span className="text-success">已完成: {completedCount}</span>
        {inProgressCount > 0 && (
          <span className="text-primary animate-pulse">进行中: {inProgressCount}</span>
        )}
        <span className="ml-auto">{progress}%</span>
      </div>
    </>
  );

  if (collapsible) {
    return (
      <div className="rounded-[12px] border border-border bg-background p-4">
        <button
          type="button"
          onClick={() => setIsCollapsed(!isCollapsed)}
          className="flex w-full items-center justify-between"
        >
          <div className="flex items-center gap-2">
            <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
              PLAN
            </span>
            <span className="font-medium text-foreground">执行计划</span>
          </div>
          <span className="text-xs text-muted-foreground">
            {isCollapsed ? "展开" : "收起"}
          </span>
        </button>
        {!isCollapsed && <div className="mt-3">{cardContent}</div>}
      </div>
    );
  }

  return (
    <div className="rounded-[12px] border border-border bg-background p-4">
      <div className="mb-3 flex items-center gap-2">
        <span className="inline-flex rounded-[6px] border border-border bg-secondary px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          PLAN
        </span>
        <span className="font-medium text-foreground">执行计划</span>
      </div>
      {cardContent}
    </div>
  );
});

function PlanStepItem({
  step,
  index,
}: {
  step: TurnPlanStep;
  index: number;
}) {
  const statusConfig = getStatusConfig(step.status);

  return (
    <li className="flex items-start gap-3 rounded-[10px] border border-border/70 bg-secondary/35 px-3 py-2.5">
      <span
        className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-[6px] text-xs ${statusConfig.bgClass}`}
      >
        {statusConfig.icon}
      </span>

      <div className="flex-1 min-w-0">
        <p
          className={`text-sm ${
            step.status === "completed"
              ? "text-muted-foreground line-through"
              : "text-foreground"
          }`}
        >
          {step.step}
        </p>
      </div>

      <span className="text-xs text-muted-foreground">#{index + 1}</span>
    </li>
  );
}

function getStatusConfig(status: TurnPlanStepStatus): {
  icon: string;
  bgClass: string;
} {
  switch (status) {
    case "pending":
      return { icon: "○", bgClass: "bg-background text-muted-foreground border border-border" };
    case "inProgress":
      return { icon: "⋯", bgClass: "bg-primary/10 text-primary border border-primary/20" };
    case "completed":
      return { icon: "✓", bgClass: "bg-success/10 text-success border border-success/20" };
    default:
      return { icon: "?", bgClass: "bg-background text-muted-foreground border border-border" };
  }
}

export default AcpPlanCard;
