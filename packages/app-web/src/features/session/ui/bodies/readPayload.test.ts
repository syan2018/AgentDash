import { describe, it, expect } from "vitest";
import { parseReadToolText } from "./readPayload";

describe("parseReadToolText", () => {
  it("parses file: header and N | line numbers", () => {
    const raw = `file: packages/app-web/src/foo.ts
   1 | import React from "react";
   2 | export const Foo = () => <div />;`;

    const result = parseReadToolText(raw, 1);
    expect(result.filePath).toBe("packages/app-web/src/foo.ts");
    expect(result.parsedLineNumbers).toBe(true);
    expect(result.lines).toHaveLength(2);
    expect(result.lines[0]).toEqual({ lineNo: 1, text: 'import React from "react";' });
    expect(result.lines[1]).toEqual({ lineNo: 2, text: "export const Foo = () => <div />;" });
    expect(result.bodyText).not.toContain("file:");
    expect(result.bodyText).not.toMatch(/^\s*\d+\s*\|/m);
  });

  it("parses line numbers without file: header", () => {
    const raw = `   10 | function hello() {
   11 |   return "world";
   12 | }`;

    const result = parseReadToolText(raw, 1);
    expect(result.filePath).toBeUndefined();
    expect(result.parsedLineNumbers).toBe(true);
    expect(result.lines[0]).toEqual({ lineNo: 10, text: "function hello() {" });
    expect(result.lines[1]).toEqual({ lineNo: 11, text: '  return "world";' });
    expect(result.lines[2]).toEqual({ lineNo: 12, text: "}" });
  });

  it("falls back to sequential numbering when no line numbers", () => {
    const raw = "just some plain text\nsecond line";

    const result = parseReadToolText(raw, 5);
    expect(result.filePath).toBeUndefined();
    expect(result.parsedLineNumbers).toBe(false);
    expect(result.lines[0]).toEqual({ lineNo: 5, text: "just some plain text" });
    expect(result.lines[1]).toEqual({ lineNo: 6, text: "second line" });
  });

  it("handles empty lines within line-numbered text", () => {
    const raw = `file: test.ts
   1 | line one
   2 |
   3 | line three`;

    const result = parseReadToolText(raw, 1);
    expect(result.parsedLineNumbers).toBe(true);
    expect(result.lines).toHaveLength(3);
    expect(result.lines[1].lineNo).toBe(2);
    expect(result.lines[1].text).toBe("");
  });

  it("handles wide line numbers", () => {
    const raw = `file: big.ts
 100 | first
 101 | second
 102 | third`;

    const result = parseReadToolText(raw, 1);
    expect(result.parsedLineNumbers).toBe(true);
    expect(result.lines[0]).toEqual({ lineNo: 100, text: "first" });
    expect(result.lines[2]).toEqual({ lineNo: 102, text: "third" });
  });

  it("handles no-space format: 1|text", () => {
    const raw = `1|hello
2|world`;

    const result = parseReadToolText(raw, 1);
    expect(result.parsedLineNumbers).toBe(true);
    expect(result.lines[0]).toEqual({ lineNo: 1, text: "hello" });
    expect(result.lines[1]).toEqual({ lineNo: 2, text: "world" });
  });

  it("preserves raw text for copy-raw", () => {
    const raw = `file: foo.ts
   1 | hello`;

    const result = parseReadToolText(raw, 1);
    expect(result.rawText).toBe(raw);
  });
});
