import { describe, it, expect } from "vitest";
import { parseUnifiedDiff, synthesizeFromOldNew } from "./diffPayload";

describe("parseUnifiedDiff", () => {
  it("parses a standard unified diff", () => {
    const diff = `--- a/foo.ts
+++ b/foo.ts
@@ -1,3 +1,3 @@
 line1
-old line
+new line
 line3`;

    const result = parseUnifiedDiff(diff);
    expect(result.added).toBe(1);
    expect(result.removed).toBe(1);
    expect(result.lines.some((l) => l.kind === "meta")).toBe(true);
    expect(result.lines.some((l) => l.kind === "hunk")).toBe(true);
    expect(result.lines.filter((l) => l.kind === "add")).toHaveLength(1);
    expect(result.lines.filter((l) => l.kind === "remove")).toHaveLength(1);
    expect(result.lines.filter((l) => l.kind === "context")).toHaveLength(2);
  });

  it("handles diff without hunk headers (pure +/-)", () => {
    const diff = `-removed
+added`;

    const result = parseUnifiedDiff(diff);
    expect(result.added).toBe(1);
    expect(result.removed).toBe(1);
  });

  it("returns empty for empty input", () => {
    const result = parseUnifiedDiff("");
    expect(result.lines).toEqual([]);
    expect(result.added).toBe(0);
    expect(result.removed).toBe(0);
  });

  it("tracks line numbers from hunk header", () => {
    const diff = `@@ -10,2 +20,2 @@
-old
+new
 ctx`;

    const result = parseUnifiedDiff(diff);
    const removeLine = result.lines.find((l) => l.kind === "remove");
    const addLine = result.lines.find((l) => l.kind === "add");
    const ctxLine = result.lines.find((l) => l.kind === "context");

    expect(removeLine?.kind === "remove" && removeLine.oldNo).toBe(10);
    expect(addLine?.kind === "add" && addLine.newNo).toBe(20);
    expect(ctxLine?.kind === "context" && ctxLine.oldNo).toBe(11);
    expect(ctxLine?.kind === "context" && ctxLine.newNo).toBe(21);
  });
});

describe("synthesizeFromOldNew", () => {
  it("generates remove + add lines", () => {
    const result = synthesizeFromOldNew("old", "new");
    expect(result.removed).toBe(1);
    expect(result.added).toBe(1);
    expect(result.lines).toHaveLength(2);
    expect(result.lines[0]).toEqual({ kind: "remove", oldNo: 1, newNo: null, text: "old" });
    expect(result.lines[1]).toEqual({ kind: "add", oldNo: null, newNo: 1, text: "new" });
  });

  it("handles empty old (new file)", () => {
    const result = synthesizeFromOldNew("", "new content");
    expect(result.removed).toBe(0);
    expect(result.added).toBe(1);
  });

  it("handles empty new (delete)", () => {
    const result = synthesizeFromOldNew("old content", "");
    expect(result.removed).toBe(1);
    expect(result.added).toBe(0);
  });

  it("handles both empty", () => {
    const result = synthesizeFromOldNew("", "");
    expect(result.lines).toEqual([]);
    expect(result.added).toBe(0);
    expect(result.removed).toBe(0);
  });
});
