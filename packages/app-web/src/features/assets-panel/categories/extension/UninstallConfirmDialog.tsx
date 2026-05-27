/**
 * UninstallConfirmDialog — 卸载已安装扩展前的二次确认。
 *
 * 仅删 installation；归档（package_artifact）保留以便重装。
 */

import { useState } from "react";

import { ConfirmDialog } from "@agentdash/ui";

interface Props {
  open: boolean;
  installationName: string;
  extensionKey: string;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}

export function UninstallConfirmDialog({
  open,
  installationName,
  extensionKey,
  onClose,
  onConfirm,
}: Props) {
  const [busy, setBusy] = useState(false);

  return (
    <ConfirmDialog
      open={open}
      tone="danger"
      title="卸载扩展"
      description={`卸载「${installationName}」（${extensionKey}）？这会移除项目下该扩展的安装记录；归档保留，可随时从「归档」段重新安装。`}
      confirmLabel="卸载"
      isConfirming={busy}
      onClose={busy ? () => undefined : onClose}
      onConfirm={() => {
        setBusy(true);
        void onConfirm()
          .catch(() => undefined)
          .finally(() => setBusy(false));
      }}
    />
  );
}

export default UninstallConfirmDialog;
