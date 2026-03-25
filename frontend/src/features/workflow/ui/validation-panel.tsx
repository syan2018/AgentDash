import type { ValidationIssue } from "../../../types";

export function ValidationPanel({ issues }: { issues: ValidationIssue[] }) {
  const errors = issues.filter((item) => item.severity === "error");
  const warnings = issues.filter((item) => item.severity === "warning");

  return (
    <div className="space-y-2">
      {errors.length > 0 && (
        <div className="space-y-1.5 rounded-[10px] border border-destructive/30 bg-destructive/5 px-3 py-2.5">
          <p className="text-[11px] font-medium text-destructive">{errors.length} 个错误</p>
          {errors.map((issue, index) => (
            <div key={index} className="text-[11px] leading-5 text-destructive/80">
              <span className="font-mono text-[10px] text-destructive/60">{issue.field_path}</span>
              <span className="mx-1.5">·</span>
              {issue.message}
            </div>
          ))}
        </div>
      )}
      {warnings.length > 0 && (
        <div className="space-y-1.5 rounded-[10px] border border-amber-300/30 bg-amber-500/5 px-3 py-2.5">
          <p className="text-[11px] font-medium text-amber-700">{warnings.length} 个警告</p>
          {warnings.map((issue, index) => (
            <div key={index} className="text-[11px] leading-5 text-amber-700/80">
              <span className="font-mono text-[10px] text-amber-700/60">{issue.field_path}</span>
              <span className="mx-1.5">·</span>
              {issue.message}
            </div>
          ))}
        </div>
      )}
      {errors.length === 0 && warnings.length === 0 && (
        <div className="rounded-[10px] border border-emerald-300/30 bg-emerald-500/5 px-3 py-2.5">
          <p className="text-[11px] text-emerald-700">校验通过，无错误或警告。</p>
        </div>
      )}
    </div>
  );
}
