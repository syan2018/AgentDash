import { describe, it, expect } from "vitest";
import { normalizeDynamicOutput, normalizeMcpOutput } from "./toolOutputContent";

describe("normalizeDynamicOutput", () => {
  it("converts inputText to text block", () => {
    const result = normalizeDynamicOutput([
      { type: "inputText", text: "hello world" },
    ]);
    expect(result).toEqual([{ kind: "text", text: "hello world" }]);
  });

  it("converts inputImage to image block", () => {
    const result = normalizeDynamicOutput([
      { type: "inputImage", imageUrl: "https://example.com/img.png" },
    ]);
    expect(result).toEqual([
      { kind: "image", imageUrl: "https://example.com/img.png" },
    ]);
  });

  it("merges adjacent text blocks", () => {
    const result = normalizeDynamicOutput([
      { type: "inputText", text: "line1" },
      { type: "inputText", text: "line2" },
    ]);
    expect(result).toEqual([{ kind: "text", text: "line1\nline2" }]);
  });

  it("does not merge text blocks separated by image", () => {
    const result = normalizeDynamicOutput([
      { type: "inputText", text: "before" },
      { type: "inputImage", imageUrl: "img.png" },
      { type: "inputText", text: "after" },
    ]);
    expect(result).toHaveLength(3);
    expect(result[0]).toEqual({ kind: "text", text: "before" });
    expect(result[1]).toEqual({ kind: "image", imageUrl: "img.png" });
    expect(result[2]).toEqual({ kind: "text", text: "after" });
  });

  it("returns empty for null/undefined", () => {
    expect(normalizeDynamicOutput(null)).toEqual([]);
    expect(normalizeDynamicOutput(undefined)).toEqual([]);
    expect(normalizeDynamicOutput([])).toEqual([]);
  });
});

describe("normalizeMcpOutput", () => {
  it("converts MCP text block", () => {
    const result = normalizeMcpOutput([{ type: "text", text: "mcp text" }]);
    expect(result).toEqual([{ kind: "text", text: "mcp text" }]);
  });

  it("converts MCP image block to data URL", () => {
    const result = normalizeMcpOutput([
      { type: "image", data: "AAAA", mimeType: "image/jpeg" },
    ]);
    expect(result).toEqual([
      { kind: "image", imageUrl: "data:image/jpeg;base64,AAAA" },
    ]);
  });

  it("converts MCP resource block", () => {
    const result = normalizeMcpOutput([
      { type: "resource", resource: { uri: "file:///foo.txt", text: "content" } },
    ]);
    expect(result).toEqual([
      { kind: "resource", uri: "file:///foo.txt", text: "content" },
    ]);
  });

  it("converts MCP resource_link block", () => {
    const result = normalizeMcpOutput([
      { type: "resource_link", uri: "file:///bar.txt", name: "bar" },
    ]);
    expect(result).toEqual([
      { kind: "resource", uri: "file:///bar.txt", label: "bar" },
    ]);
  });

  it("falls back to json for unknown types", () => {
    const result = normalizeMcpOutput([{ type: "custom", data: 123 }]);
    expect(result).toEqual([
      { kind: "json", value: { type: "custom", data: 123 } },
    ]);
  });

  it("returns empty for null/undefined", () => {
    expect(normalizeMcpOutput(null)).toEqual([]);
    expect(normalizeMcpOutput(undefined)).toEqual([]);
  });

  it("merges adjacent MCP text blocks", () => {
    const result = normalizeMcpOutput([
      { type: "text", text: "a" },
      { type: "text", text: "b" },
    ]);
    expect(result).toEqual([{ kind: "text", text: "a\nb" }]);
  });
});
