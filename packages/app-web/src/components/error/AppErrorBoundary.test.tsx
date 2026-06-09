import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { AppErrorBoundary } from "./AppErrorBoundary";

// 项目测试无 DOM 环境，renderToStaticMarkup 不会经错误边界恢复，
// 因此直接验证状态转换 + fallback 渲染路径（render() 在 error 态返回崩溃屏）。

describe("AppErrorBoundary", () => {
  it("getDerivedStateFromError 把错误映射为 error 态", () => {
    const error = new Error("boom-marker-xyz");
    expect(AppErrorBoundary.getDerivedStateFromError(error)).toEqual({ error });
  });

  it("error 态渲染崩溃屏（标题 + 错误信息 + 重载入口）", () => {
    const boundary = new AppErrorBoundary({ children: null });
    boundary.state = { error: new Error("boom-marker-xyz") };
    const html = renderToStaticMarkup(<>{boundary.render()}</>);

    expect(html).toContain("应用遇到错误");
    expect(html).toContain("boom-marker-xyz");
    expect(html).toContain("重载应用");
  });

  it("自定义 title 透传到崩溃屏", () => {
    const boundary = new AppErrorBoundary({ children: null, title: "此页面出错了" });
    boundary.state = { error: new Error("x") };
    const html = renderToStaticMarkup(<>{boundary.render()}</>);

    expect(html).toContain("此页面出错了");
  });

  it("无错误时透传 children", () => {
    const boundary = new AppErrorBoundary({
      children: <div>ok-content-marker</div>,
    });
    boundary.state = { error: null };
    const html = renderToStaticMarkup(<>{boundary.render()}</>);

    expect(html).toContain("ok-content-marker");
    expect(html).not.toContain("应用遇到错误");
  });
});
