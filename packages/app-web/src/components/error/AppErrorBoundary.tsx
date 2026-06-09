import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button, StatusScreen } from "@agentdash/ui";

interface AppErrorBoundaryProps {
  children: ReactNode;
  /** 标题文案，默认“应用遇到错误” */
  title?: string;
  /**
   * 任一 key 变化时自动清除错误并重新渲染 children。
   * 路由级用法传 [pathname]，使导航离开出错页即可恢复，无需整页重载。
   */
  resetKeys?: ReadonlyArray<unknown>;
}

interface AppErrorBoundaryState {
  error: Error | null;
}

function resetKeysChanged(
  prev: ReadonlyArray<unknown> | undefined,
  next: ReadonlyArray<unknown> | undefined,
): boolean {
  if (prev === next) return false;
  if (!prev || !next || prev.length !== next.length) return true;
  return prev.some((value, index) => !Object.is(value, next[index]));
}

/**
 * 顶层 / 路由级错误边界：捕获 React 渲染期崩溃，渲染品牌化崩溃屏，
 * 避免白屏或裸浏览器报错。仅兜 render-throw；异步/fetch 错误仍由各 store 处理。
 */
export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[AppErrorBoundary]", error, info.componentStack);
  }

  componentDidUpdate(prevProps: AppErrorBoundaryProps) {
    if (
      this.state.error &&
      resetKeysChanged(prevProps.resetKeys, this.props.resetKeys)
    ) {
      this.setState({ error: null });
    }
  }

  private readonly handleReload = () => {
    window.location.reload();
  };

  render() {
    const { error } = this.state;
    if (error) {
      return (
        <StatusScreen
          tone="danger"
          title={this.props.title ?? "应用遇到错误"}
          description={error.message || "渲染时发生异常，可尝试重载。"}
          action={
            <Button variant="secondary" onClick={this.handleReload}>
              重载应用
            </Button>
          }
        />
      );
    }
    return this.props.children;
  }
}
