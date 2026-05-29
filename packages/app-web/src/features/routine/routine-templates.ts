import type { RoutineFormState } from "./form-state";

export interface RoutineTemplate {
  name: string;
  description: string;
  prompt: string;
  trigger_type: "scheduled" | "webhook";
  cron_expression: string;
}

export const ROUTINE_TEMPLATES: RoutineTemplate[] = [
  {
    name: "每日代码审查",
    description: "每天早上检查代码变更并生成审查报告",
    prompt: "检查过去 24 小时内的代码提交，识别潜在问题并生成审查报告。重点关注：安全性、性能瓶颈和代码规范。",
    trigger_type: "scheduled",
    cron_expression: "0 9 * * *",
  },
  {
    name: "PR 自动 Review",
    description: "新 PR 时自动进行代码审查",
    prompt: "对新提交的 Pull Request 进行代码审查，检查代码质量、安全性和性能问题，给出改进建议。",
    trigger_type: "webhook",
    cron_expression: "",
  },
  {
    name: "定时进度报告",
    description: "每周五下午汇总本周项目进展",
    prompt: "汇总本周项目进展，包括已完成任务、进行中任务和阻塞项，生成结构化周报。",
    trigger_type: "scheduled",
    cron_expression: "0 17 * * 5",
  },
  {
    name: "依赖安全扫描",
    description: "每周一扫描依赖库安全漏洞",
    prompt: "扫描项目依赖库的安全公告，报告任何新发现的 CVE 漏洞及修复建议。按严重程度分级列出。",
    trigger_type: "scheduled",
    cron_expression: "0 8 * * 1",
  },
];

export function templateToFormPatch(template: RoutineTemplate): Partial<RoutineFormState> {
  return {
    name: template.name,
    prompt_template: template.prompt,
    trigger_type: template.trigger_type,
    cron_expression: template.cron_expression || "0 9 * * *",
  };
}
