import { describe, expect, it } from "vitest";

import type { AgentPreset } from "../../../types";
import {
  addMcpPresetDirective,
  formToPreset,
  mcpCapabilityKey,
  mcpToolCapabilityPath,
  presetToForm,
  removeMcpPresetDirective,
  replaceWellKnownCapabilitySelection,
  selectedMcpPresetKeysFromDirectives,
  setMcpToolBlockedDirective,
} from "./form-state";

function presetWithConfig(config: AgentPreset["config"]): AgentPreset {
  return {
    name: "agent",
    agent_type: "codex",
    config,
  };
}

describe("ProjectAgent preset form state capability directives", () => {
  it("roundtrip 保留旧配置中的 MCP tool remove directive", () => {
    const form = presetToForm(presetWithConfig({
      capability_directives: [
        { add: "mcp:abc-config-analyzer" },
        { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
      ],
    }));

    expect(form.capability_directives).toEqual([
      { add: "mcp:abc-config-analyzer" },
      { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
    ]);

    const saved = formToPreset(form);
    expect(saved.config).toEqual({
      backend_requirement: "required",
      capability_directives: [
        { add: "mcp:abc-config-analyzer" },
        { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
      ],
    });
  });

  it("选择 MCP preset 后保存为 add directive", () => {
    const form = presetToForm(presetWithConfig({}));
    form.capability_directives = addMcpPresetDirective(
      form.capability_directives,
      "abc-config-analyzer",
    );

    expect(selectedMcpPresetKeysFromDirectives(form.capability_directives)).toEqual([
      "abc-config-analyzer",
    ]);
    expect(formToPreset(form).config).toEqual({
      backend_requirement: "required",
      capability_directives: [
        { add: "mcp:abc-config-analyzer" },
      ],
    });
  });

  it("普通工具能力编辑不会丢失 MCP tool-level remove directive", () => {
    const directives = [
      { add: "mcp:abc-config-analyzer" },
      { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
    ];

    expect(replaceWellKnownCapabilitySelection(directives, ["file_read"])).toEqual([
      { add: "mcp:abc-config-analyzer" },
      { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
      { add: "file_read" },
    ]);
  });

  it("MCP tool block helper 写入并移除 raw tool name remove path", () => {
    const blocked = setMcpToolBlockedDirective(
      addMcpPresetDirective([], "abc-config-analyzer"),
      "abc-config-analyzer",
      "ABCConfigAnalyzer_get_file_content",
      true,
    );

    expect(blocked).toEqual([
      { add: "mcp:abc-config-analyzer" },
      { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
    ]);

    expect(setMcpToolBlockedDirective(
      blocked,
      "abc-config-analyzer",
      "ABCConfigAnalyzer_get_file_content",
      false,
    )).toEqual([
      { add: "mcp:abc-config-analyzer" },
    ]);
  });

  it("取消 MCP preset 会清理对应 add 和 tool remove directives", () => {
    const directives = [
      { add: "mcp:abc-config-analyzer" },
      { remove: "mcp:abc-config-analyzer::ABCConfigAnalyzer_get_file_content" },
      { remove: "mcp:other::tool" },
    ];

    expect(removeMcpPresetDirective(directives, "abc-config-analyzer")).toEqual([
      { remove: "mcp:other::tool" },
    ]);
  });

  it("集中构造 MCP capability path", () => {
    expect(mcpCapabilityKey("abc-config-analyzer")).toBe("mcp:abc-config-analyzer");
    expect(mcpToolCapabilityPath(
      "abc-config-analyzer",
      "ABCConfigAnalyzer_inspect_json_content",
    )).toBe("mcp:abc-config-analyzer::ABCConfigAnalyzer_inspect_json_content");
    expect(() => mcpCapabilityKey("bad::key")).toThrow("MCP Preset key 非法");
    expect(() => mcpToolCapabilityPath("abc", "bad::tool")).toThrow("MCP tool name 非法");
  });

  it("backend requirement 缺省展示为 required 并参与保存", () => {
    const form = presetToForm(presetWithConfig({}));

    expect(form.backend_requirement).toBe("required");
    expect(formToPreset(form).config).toEqual({
      backend_requirement: "required",
    });
  });

  it("backend requirement 保存 optional", () => {
    const form = presetToForm(presetWithConfig({
      backend_requirement: "optional",
    }));

    expect(form.backend_requirement).toBe("optional");
    expect(formToPreset(form).config).toEqual({
      backend_requirement: "optional",
    });
  });
});
