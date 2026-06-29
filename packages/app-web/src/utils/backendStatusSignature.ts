import type { BackendConfig } from "../types";

export function backendStatusSignature(backends: BackendConfig[]): string {
  return backends
    .map((backend) =>
      [
        backend.id,
        backend.online ? "online" : "offline",
        backend.runtime_health?.status ?? "",
        backend.runtime_health?.updated_at ?? "",
      ].join(":"),
    )
    .join("|");
}
