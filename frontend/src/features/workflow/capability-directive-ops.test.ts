import { describe, expect, it } from "vitest";

import type { CapabilityDirective } from "../../types/workflow";
import { parseCapabilityPath, toQualifiedString } from "../../types/workflow";
import {
  addDirective,
  capabilityBlockedByWorkflow,
  capabilityExplicitlyAdded,
  directiveEquals,
  hasDirective,
  listDeclaredCapabilityKeys,
  makeAddCapability,
  makeAddTool,
  makeRemoveCapability,
  makeRemoveTool,
  normalizeDirectives,
  removeDirective,
  toolBlockedByWorkflow,
} from "./capability-directive-ops";

describe("CapabilityPath 序列化", () => {
  it("toQualifiedString 输出短 path", () => {
    expect(toQualifiedString({ capability: "file_read", tool: null })).toBe("file_read");
  });

  it("toQualifiedString 输出长 path", () => {
    expect(toQualifiedString({ capability: "file_read", tool: "fs_grep" })).toBe(
      "file_read::fs_grep",
    );
  });

  it("parseCapabilityPath 解析短 path", () => {
    expect(parseCapabilityPath("file_read")).toEqual({ capability: "file_read", tool: null });
  });

  it("parseCapabilityPath 解析长 path", () => {
    expect(parseCapabilityPath("file_read::fs_grep")).toEqual({
      capability: "file_read",
      tool: "fs_grep",
    });
  });

  it("parseCapabilityPath 解析 mcp 短 path", () => {
    expect(parseCapabilityPath("mcp:code_analyzer")).toEqual({
      capability: "mcp:code_analyzer",
      tool: null,
    });
  });

  it("parseCapabilityPath 解析 mcp 长 path", () => {
    expect(parseCapabilityPath("mcp:workflow_management::upsert")).toEqual({
      capability: "mcp:workflow_management",
      tool: "upsert",
    });
  });

  it("parseCapabilityPath 拒绝空字符串", () => {
    expect(() => parseCapabilityPath("")).toThrow();
    expect(() => parseCapabilityPath("   ")).toThrow();
  });

  it("parseCapabilityPath 拒绝多级嵌套", () => {
    expect(() => parseCapabilityPath("a::b::c")).toThrow();
  });

  it("parseCapabilityPath 拒绝两边为空的长 path", () => {
    expect(() => parseCapabilityPath("::tool")).toThrow();
    expect(() => parseCapabilityPath("cap::")).toThrow();
  });
});

describe("Directive 构造器", () => {
  it("makeAddCapability 产出短 path Add", () => {
    expect(makeAddCapability("file_read")).toEqual({ add: "file_read" });
  });

  it("makeRemoveCapability 产出短 path Remove", () => {
    expect(makeRemoveCapability("shell_execute")).toEqual({ remove: "shell_execute" });
  });

  it("makeAddTool 产出长 path Add", () => {
    expect(makeAddTool("file_read", "fs_read")).toEqual({ add: "file_read::fs_read" });
  });

  it("makeRemoveTool 产出长 path Remove", () => {
    expect(makeRemoveTool("file_read", "fs_grep")).toEqual({ remove: "file_read::fs_grep" });
  });
});

describe("Directive 集合操作", () => {
  it("directiveEquals 判定同值", () => {
    expect(directiveEquals({ add: "a" }, { add: "a" })).toBe(true);
    expect(directiveEquals({ add: "a" }, { remove: "a" })).toBe(false);
    expect(directiveEquals({ add: "a" }, { add: "b" })).toBe(false);
  });

  it("hasDirective 查找", () => {
    const list: CapabilityDirective[] = [{ add: "file_read" }, { remove: "shell_execute" }];
    expect(hasDirective(list, { add: "file_read" })).toBe(true);
    expect(hasDirective(list, { remove: "file_read" })).toBe(false);
  });

  it("addDirective 不重复追加", () => {
    const list: CapabilityDirective[] = [{ add: "file_read" }];
    expect(addDirective(list, { add: "file_read" })).toBe(list);
    const next = addDirective(list, { remove: "shell_execute" });
    expect(next).toHaveLength(2);
  });

  it("removeDirective 删除所有匹配", () => {
    const list: CapabilityDirective[] = [
      { add: "file_read" },
      { remove: "shell_execute" },
      { add: "file_read" },
    ];
    const next = removeDirective(list, { add: "file_read" });
    expect(next).toEqual([{ remove: "shell_execute" }]);
  });

  it("normalizeDirectives 保留后出现的那条", () => {
    const list: CapabilityDirective[] = [
      { add: "file_read" },
      { add: "canvas" },
      { add: "file_read" },
    ];
    const next = normalizeDirectives(list);
    expect(next).toEqual([{ add: "canvas" }, { add: "file_read" }]);
  });
});

describe("屏蔽 / 声明状态查询", () => {
  it("capabilityBlockedByWorkflow 命中短 path Remove", () => {
    const list: CapabilityDirective[] = [{ remove: "shell_execute" }];
    expect(capabilityBlockedByWorkflow(list, "shell_execute")).toBe(true);
    expect(capabilityBlockedByWorkflow(list, "file_read")).toBe(false);
  });

  it("capabilityBlockedByWorkflow 不把工具级 Remove 当成能力屏蔽", () => {
    const list: CapabilityDirective[] = [{ remove: "file_read::fs_grep" }];
    expect(capabilityBlockedByWorkflow(list, "file_read")).toBe(false);
  });

  it("toolBlockedByWorkflow 命中长 path Remove", () => {
    const list: CapabilityDirective[] = [{ remove: "file_read::fs_grep" }];
    expect(toolBlockedByWorkflow(list, "file_read", "fs_grep")).toBe(true);
    expect(toolBlockedByWorkflow(list, "file_read", "fs_read")).toBe(false);
  });

  it("toolBlockedByWorkflow 不把能力级 Remove 当成工具屏蔽", () => {
    const list: CapabilityDirective[] = [{ remove: "file_read" }];
    expect(toolBlockedByWorkflow(list, "file_read", "fs_grep")).toBe(false);
  });

  it("capabilityExplicitlyAdded 识别短 path Add", () => {
    const list: CapabilityDirective[] = [{ add: "workflow_management" }];
    expect(capabilityExplicitlyAdded(list, "workflow_management")).toBe(true);
    expect(capabilityExplicitlyAdded(list, "shell_execute")).toBe(false);
  });

  it("capabilityExplicitlyAdded 不把长 path Add 当成能力级 Add", () => {
    const list: CapabilityDirective[] = [{ add: "file_read::fs_read" }];
    expect(capabilityExplicitlyAdded(list, "file_read")).toBe(false);
  });

  it("listDeclaredCapabilityKeys 合并短 path 与长 path", () => {
    const list: CapabilityDirective[] = [
      { add: "workflow_management" },
      { add: "file_read::fs_read" },
      { remove: "shell_execute" },
    ];
    const keys = listDeclaredCapabilityKeys(list);
    expect(keys).toContain("workflow_management");
    expect(keys).toContain("file_read");
    // Remove 不算 declared add
    expect(keys).not.toContain("shell_execute");
  });
});
