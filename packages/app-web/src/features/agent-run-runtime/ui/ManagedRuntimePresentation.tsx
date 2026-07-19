import { useState } from "react";

import type {
  ManagedRuntimeInteractionResponse,
  ManagedRuntimePresentationContentBlock,
} from "../../../generated/agent-runtime-contracts";
import {
  respondAgentRunInteraction,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";
import type {
  AgentRunRuntimeInteraction,
  AgentRunRuntimeItem,
} from "../model/useAgentRunRuntimeFeed";

function json(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

function contentText(block: ManagedRuntimePresentationContentBlock): string {
  switch (block.kind) {
    case "text":
      return block.text;
    case "image":
      return `[image ${block.media_type}] ${block.source}`;
    case "local_resource":
      return block.path;
    case "resource_link":
      return block.title ? `${block.title} · ${block.uri}` : block.uri;
    case "skill_reference":
      return block.path ? `${block.name} · ${block.path}` : block.name;
    case "mention":
      return `${block.label} · ${block.reference}`;
    case "structured":
      return json(block.value);
  }
}

function contentList(
  blocks: readonly ManagedRuntimePresentationContentBlock[],
): string {
  return blocks.map(contentText).filter(Boolean).join("\n");
}

function bodyPresentation(item: AgentRunRuntimeItem): {
  label: string;
  headline?: string;
  detail: string;
} {
  const body = item.presentation.body;
  switch (body.kind) {
    case "user_message":
      return { label: "用户", detail: contentList(body.content) };
    case "hook_prompt":
      return {
        label: `Hook · ${body.hook_point}`,
        detail: contentList(body.content),
      };
    case "agent_message":
      return {
        label: body.phase ? `Agent · ${body.phase}` : "Agent",
        detail: contentList(body.content),
      };
    case "reasoning":
      return {
        label: "推理",
        headline: contentList(body.summary),
        detail: contentList(body.content),
      };
    case "plan":
      return {
        label: "计划",
        headline: body.explanation ?? undefined,
        detail: body.steps
          .map((step) => `[${step.status}] ${step.text}`)
          .join("\n"),
      };
    case "command_execution":
      return {
        label: "命令",
        headline: body.command,
        detail: [
          body.cwd ? `cwd: ${body.cwd}` : "",
          ...body.output.map((output) => `[${output.stream}] ${output.text}`),
        ]
          .filter(Boolean)
          .join("\n"),
      };
    case "file_change":
      return {
        label: "文件变更",
        detail: [
          ...body.changes.map((change) => `${change.change_kind}: ${change.path}\n${change.patch}`),
          contentList(body.output),
        ]
          .filter(Boolean)
          .join("\n"),
      };
    case "file_read":
      return {
        label: "文件读取",
        headline: body.path,
        detail: contentList(body.content),
      };
    case "file_search":
      return {
        label: `文件搜索 · ${body.mode}`,
        headline: body.query,
        detail: body.matches
          .map((match) => `${match.path}${match.line == null ? "" : `:${match.line}`} ${match.preview ?? ""}`)
          .join("\n"),
      };
    case "mcp_tool_call":
      return {
        label: "MCP 工具",
        headline: `${body.server}/${body.tool}`,
        detail: json({
          arguments: body.arguments,
          progress: contentList(body.progress),
          result: body.result,
        }),
      };
    case "dynamic_tool_call":
      return {
        label: "动态工具",
        headline: [body.namespace, body.tool].filter(Boolean).join("/"),
        detail: json({
          arguments: body.arguments,
          progress: contentList(body.progress),
          result: body.result,
        }),
      };
    case "collaboration_tool_call":
      return {
        label: "协作",
        headline: [body.action, body.target].filter(Boolean).join(" · "),
        detail: [body.prompt, body.result == null ? "" : json(body.result)]
          .filter(Boolean)
          .join("\n"),
      };
    case "subagent_activity":
      return {
        label: `子代理 · ${body.status}`,
        headline: body.task,
        detail: contentList(body.result),
      };
    case "web_search":
      return {
        label: `网页搜索 · ${body.action}`,
        headline: body.query ?? body.url ?? undefined,
        detail: contentList(body.results),
      };
    case "image_view":
      return {
        label: "查看图片",
        headline: body.path,
        detail: body.detail ?? "",
      };
    case "image_generation":
      return {
        label: "生成图片",
        headline: body.prompt,
        detail: [body.revised_prompt, contentList(body.outputs)]
          .filter(Boolean)
          .join("\n"),
      };
    case "sleep":
      return {
        label: "等待",
        detail: `${body.duration_ms.toString()} ms`,
      };
    case "review":
      return {
        label: "审查",
        headline: body.summary ?? undefined,
        detail: body.findings
          .map((finding) => `[${finding.severity}] ${finding.title}\n${finding.body}`)
          .join("\n"),
      };
    case "terminal_control":
      return {
        label: "终端控制",
        headline: `${body.terminal_id} · ${body.action}`,
        detail: body.text ?? "",
      };
    case "context_compaction":
      return {
        label: "上下文压缩",
        headline: body.source_digest ?? undefined,
        detail: body.summary ? contentList(body.summary) : "",
      };
    case "generic_tool_activity":
      return {
        label: "通用工具活动",
        headline: body.name,
        detail: json({
          arguments: body.arguments,
          progress: contentList(body.progress),
          result: body.result,
        }),
      };
    case "error":
      return {
        label: `错误 · ${body.code}`,
        headline: body.message,
        detail: body.details ? contentList(body.details) : "",
      };
  }
}

function outcomeClass(outcome: string): string {
  switch (outcome) {
    case "failed":
    case "lost":
      return "text-destructive";
    case "interrupted":
      return "text-warning";
    case "completed":
      return "text-success";
    default:
      return "text-muted-foreground";
  }
}

export function ManagedRuntimeItemView({
  item,
  isStreaming = false,
}: {
  item: AgentRunRuntimeItem;
  isStreaming?: boolean;
}) {
  const presentation = bodyPresentation(item);
  const outcome = item.presentation.terminal?.outcome ?? item.status;
  const diagnostic = item.presentation.terminal?.error;
  return (
    <article
      data-runtime-item-kind={item.presentation.body.kind}
      className="rounded-[10px] border border-border bg-background px-3 py-2.5"
    >
      <header className="flex items-center justify-between gap-3 text-[11px]">
        <span className="font-semibold uppercase tracking-wide text-muted-foreground">
          {presentation.label}
        </span>
        <span className={outcomeClass(outcome)}>
          {isStreaming ? "streaming" : outcome}
        </span>
      </header>
      {presentation.headline && (
        <div className="mt-1.5 break-words text-sm font-medium text-foreground">
          {presentation.headline}
        </div>
      )}
      {presentation.detail && (
        <pre className="mt-1.5 whitespace-pre-wrap break-words font-sans text-sm leading-6 text-foreground/90">
          {presentation.detail}
        </pre>
      )}
      {diagnostic && (
        <div className="mt-2 rounded-[6px] bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {diagnostic.code}: {diagnostic.message}
        </div>
      )}
    </article>
  );
}

function interactionPresentation(interaction: AgentRunRuntimeInteraction) {
  const request = interaction.request;
  switch (request.kind) {
    case "approval":
      return {
        label: "审批",
        prompt: request.prompt,
        detail: request.reason ?? (request.proposed_action == null ? "" : json(request.proposed_action)),
      };
    case "user_input":
      return {
        label: "用户输入",
        prompt: request.prompt,
        detail: request.questions.map((question) => question.prompt).join("\n"),
      };
    case "mcp_elicitation":
      return {
        label: `MCP 请求 · ${request.server}`,
        prompt: request.prompt,
        detail: json(request.schema),
      };
    case "dynamic_tool":
      return {
        label: "动态工具请求",
        prompt: request.prompt,
        detail: `${[request.namespace, request.tool].filter(Boolean).join("/")}\n${json(request.arguments)}`,
      };
  }
}

export function ManagedRuntimeInteractionView({
  interaction,
  target,
  expectedRevision,
}: {
  interaction: AgentRunRuntimeInteraction;
  target: AgentRunRuntimeTarget;
  expectedRevision: bigint;
}) {
  const [input, setInput] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const presentation = interactionPresentation(interaction);

  const resolve = async (response: ManagedRuntimeInteractionResponse) => {
    setPending(true);
    setError(null);
    try {
      await respondAgentRunInteraction(
        target,
        interaction.id,
        response,
        crypto.randomUUID(),
        expectedRevision,
      );
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "提交 interaction 回应失败");
    } finally {
      setPending(false);
    }
  };

  return (
    <article className="rounded-[10px] border border-warning/30 bg-warning/5 px-3 py-2.5">
      <div className="text-[11px] font-semibold uppercase tracking-wide text-warning">
        {presentation.label} · {interaction.status}
      </div>
      <div className="mt-1 text-sm font-medium">{presentation.prompt}</div>
      {presentation.detail && (
        <pre className="mt-1 whitespace-pre-wrap break-words font-sans text-xs text-muted-foreground">
          {presentation.detail}
        </pre>
      )}
      {interaction.status === "pending" && interaction.request.kind === "approval" && (
        <div className="mt-2 flex gap-2">
          <button disabled={pending} onClick={() => void resolve({ kind: "approved" })} className="rounded-[6px] bg-primary px-2 py-1 text-xs text-primary-foreground">
            批准
          </button>
          <button disabled={pending} onClick={() => void resolve({ kind: "denied", reason: null })} className="rounded-[6px] border border-border px-2 py-1 text-xs">
            拒绝
          </button>
        </div>
      )}
      {interaction.status === "pending" && interaction.request.kind !== "approval" && (
        <div className="mt-2 space-y-2">
          <textarea value={input} onChange={(event) => setInput(event.target.value)} className="min-h-20 w-full rounded-[6px] border border-border bg-background p-2 text-xs" />
          <button
            disabled={pending || input.trim().length === 0}
            onClick={() => void resolve(
              interaction.request.kind === "user_input"
                ? { kind: "user_input", content: [{ kind: "text", text: input }] }
                : {
                    kind: "structured",
                    schema:
                      interaction.request.kind === "mcp_elicitation"
                        ? "mcp_elicitation_response"
                        : "dynamic_tool_result",
                    value: input,
                  },
            )}
            className="rounded-[6px] bg-primary px-2 py-1 text-xs text-primary-foreground"
          >
            提交
          </button>
        </div>
      )}
      {error && <div className="mt-2 text-xs text-destructive">{error}</div>}
    </article>
  );
}
