import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DisclosureChevron, DisclosureRow } from "./disclosure";

describe("DisclosureChevron", () => {
  it("keeps one fixed-size glyph and rotates it when expanded", () => {
    const collapsed = renderToStaticMarkup(<DisclosureChevron expanded={false} />);
    const expanded = renderToStaticMarkup(<DisclosureChevron expanded />);

    expect(collapsed).toContain('width="12"');
    expect(expanded).toContain('width="12"');
    expect(collapsed).toContain('d="M4.5 2.5 8 6 4.5 9.5"');
    expect(expanded).toContain('d="M4.5 2.5 8 6 4.5 9.5"');
    expect(collapsed).not.toContain("rotate-90");
    expect(expanded).toContain("rotate-90");
  });
});

describe("DisclosureRow", () => {
  it("keeps the same icon slot and content spacing in both states", () => {
    const collapsed = renderToStaticMarkup(
      <DisclosureRow expanded={false}>TOOLS</DisclosureRow>,
    );
    const expanded = renderToStaticMarkup(
      <DisclosureRow expanded>TOOLS</DisclosureRow>,
    );

    for (const html of [collapsed, expanded]) {
      expect(html).toContain("h-4 w-3");
      expect(html).toContain("items-center gap-2");
      expect(html).toContain(">TOOLS</button>");
    }
  });
});
