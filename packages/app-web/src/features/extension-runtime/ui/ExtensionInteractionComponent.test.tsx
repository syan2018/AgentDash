import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ExtensionInteractionComponent } from "./ExtensionInteractionComponent";

describe("ExtensionInteractionComponent", () => {
  it("renders an opaque-origin iframe with restrictive CSP", () => {
    const html = renderToStaticMarkup(
      <ExtensionInteractionComponent
        descriptor={{
          component_key: "demo.card",
          contract_version: 1,
          renderer: { kind: "iframe", entry: "dist/components/demo.card/index.html" },
          props_schema: true,
          events_schema: {},
          state_projection_schema: true,
          slots: [],
          sizing: { min_width: 160, min_height: 120 },
          sandbox_profile: "isolated_v1",
        }}
        artifactSrc="/api/extension-artifacts/exact-digest/component.html"
        componentInstanceId="component-1"
        props={{}}
        stateProjection={{}}
        theme="light"
        locale="zh-CN"
        onEvent={() => null}
      />,
    );

    expect(html).toContain('sandbox="allow-scripts"');
    expect(html).not.toContain("allow-same-origin");
    expect(html).toContain("connect-src &#x27;none&#x27;");
    expect(html).toContain('referrerPolicy="no-referrer"');
  });
});
