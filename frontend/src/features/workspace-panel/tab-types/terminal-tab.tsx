/* eslint-disable react-refresh/only-export-components */
import type { TabTypeDescriptor } from "../tab-type-registry";
import { TerminalIcon } from "./icons";

function TerminalPlaceholder() {
  return (
    <div className="flex h-full min-h-[200px] flex-col items-center justify-center gap-3 px-6">
      <TerminalIcon className="h-8 w-8 text-muted-foreground/40" />
      <div className="text-center">
        <p className="text-sm font-medium text-muted-foreground">终端功能即将支持</p>
        <p className="mt-1 text-xs text-muted-foreground/70">
          将支持在此面板中实时查看 shell 执行输出
        </p>
      </div>
    </div>
  );
}

export const terminalTabType: TabTypeDescriptor = {
  typeId: "terminal",
  label: "终端",
  icon: TerminalIcon,
  allowMultiple: true,
  pinned: false,

  renderContent: () => <TerminalPlaceholder />,

  resolveTitle: (uri) => {
    const id = uri.replace("terminal://", "");
    return id ? `终端: ${id}` : "终端";
  },

  parseUri: (uri) => {
    const terminalId = uri.replace("terminal://", "");
    return terminalId ? { terminalId } : null;
  },

  buildUri: ({ terminalId }) => `terminal://${terminalId ?? "new"}`,
  defaultUri: "terminal://new",
  menuOrder: 30,
};
