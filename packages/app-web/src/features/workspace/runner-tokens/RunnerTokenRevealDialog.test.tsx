import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RunnerTokenRevealDialog } from "./RunnerTokenRevealDialog";

describe("RunnerTokenRevealDialog", () => {
  it("renders the one-time plaintext token and an assembled setup command", () => {
    const html = renderToStaticMarkup(
      <RunnerTokenRevealDialog
        plaintextToken="rrt_plain_visible_once"
        tokenName="build-server-01"
        mode="create"
        onClose={() => {}}
      />,
    );

    // plaintext token is shown in this one-time dialog
    expect(html).toContain("rrt_plain_visible_once");
    // and embedded into the copyable setup command
    expect(html).toContain("agentdash-local setup");
    expect(html).toContain("--registration-token rrt_plain_visible_once");
    expect(html).toContain("--runner-name build-server-01");
    expect(html).toContain("--install-service --start");
    // it must warn that the token will not be shown again
    expect(html).toContain("此令牌不会再次展示");
  });

  it("labels rotation differently from creation", () => {
    const html = renderToStaticMarkup(
      <RunnerTokenRevealDialog
        plaintextToken="rrt_rotated"
        tokenName="build-server-01"
        mode="rotate"
        onClose={() => {}}
      />,
    );

    expect(html).toContain("已轮换");
  });
});
