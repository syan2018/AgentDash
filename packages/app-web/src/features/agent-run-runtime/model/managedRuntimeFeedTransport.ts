import type { ManagedRuntimeChangePage } from "../../../generated/agent-runtime-validators";
import {
  fetchManagedRuntimeChangePage,
  type AgentRunRuntimeTarget,
} from "../../../services/agentRunRuntime";

const POLL_INTERVAL_MS = 500;

export type ManagedRuntimeFeedLifecycle =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed";

export interface ManagedRuntimeFeedTransportOptions {
  agentRunTarget: AgentRunRuntimeTarget;
  after: bigint;
  onPage: (page: ManagedRuntimeChangePage) => void;
  onLifecycleChange: (lifecycle: ManagedRuntimeFeedLifecycle) => void;
  onError: (error: Error) => void;
}

export interface ManagedRuntimeFeedTransport {
  close: () => void;
}

function normalizeError(error: unknown): Error {
  return error instanceof Error ? error : new Error("Managed Runtime change 读取失败");
}

class PollingManagedRuntimeFeedTransport implements ManagedRuntimeFeedTransport {
  private closed = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private after: bigint;
  private connected = false;
  private readonly options: ManagedRuntimeFeedTransportOptions;

  constructor(options: ManagedRuntimeFeedTransportOptions) {
    this.options = options;
    this.after = options.after;
    void this.poll();
  }

  private async poll(): Promise<void> {
    if (this.closed) return;
    this.options.onLifecycleChange(this.connected ? "connected" : "connecting");
    try {
      const page = await fetchManagedRuntimeChangePage(
        this.options.agentRunTarget,
        this.after,
      );
      if (this.closed) return;
      this.connected = true;
      this.after = page.next;
      this.options.onLifecycleChange("connected");
      this.options.onPage(page);
    } catch (error) {
      if (this.closed) return;
      this.options.onLifecycleChange("reconnecting");
      this.options.onError(normalizeError(error));
    }
    if (!this.closed) {
      this.timer = setTimeout(() => {
        this.timer = null;
        void this.poll();
      }, POLL_INTERVAL_MS);
    }
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    this.options.onLifecycleChange("closed");
  }
}

export function createManagedRuntimeFeedTransport(
  options: ManagedRuntimeFeedTransportOptions,
): ManagedRuntimeFeedTransport {
  return new PollingManagedRuntimeFeedTransport(options);
}
