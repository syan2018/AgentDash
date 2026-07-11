import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ExecutorSelector } from "./ExecutorSelector";

describe("ExecutorSelector", () => {
  it("keeps unavailable execution profiles visible with their reason", () => {
    const html = renderToStaticMarkup(
      <ExecutorSelector
        executors={[{
          id: "PI_AGENT",
          name: "Managed Agent",
          available: false,
          unavailable_reason: "没有可执行的 LLM Provider",
        }]}
        isLoading={false}
        error={null}
        discoveredOptions={null}
        discoveredError={null}
        isDiscoveredLoading={false}
        onDiscoveredReconnect={vi.fn()}
        executor=""
        providerId=""
        modelId=""
        thinkingLevel=""
        permissionPolicy=""
        onExecutorChange={vi.fn()}
        onProviderIdChange={vi.fn()}
        onModelIdChange={vi.fn()}
        onThinkingLevelChange={vi.fn()}
        onPermissionPolicyChange={vi.fn()}
        onReset={vi.fn()}
        onRefetch={vi.fn()}
        defaultExpanded
      />,
    );

    expect(html).toContain("Managed Agent（不可用：没有可执行的 LLM Provider）");
    expect(html).toContain('value="PI_AGENT" disabled=""');
  });

  it("shows Native and Codex execution profiles as distinct product choices", () => {
    const html = renderToStaticMarkup(
      <ExecutorSelector
        executors={[
          { id: "PI_AGENT", name: "Managed Agent", available: true },
          { id: "CODEX", name: "Codex App Server", available: true },
        ]}
        isLoading={false}
        error={null}
        discoveredOptions={null}
        discoveredError={null}
        isDiscoveredLoading={false}
        onDiscoveredReconnect={vi.fn()}
        executor=""
        providerId=""
        modelId=""
        thinkingLevel=""
        permissionPolicy=""
        onExecutorChange={vi.fn()}
        onProviderIdChange={vi.fn()}
        onModelIdChange={vi.fn()}
        onThinkingLevelChange={vi.fn()}
        onPermissionPolicyChange={vi.fn()}
        onReset={vi.fn()}
        onRefetch={vi.fn()}
        defaultExpanded
      />,
    );
    expect(html).toContain('value="PI_AGENT"');
    expect(html).toContain('value="CODEX"');
  });
});
