import type { AgentContextSnapshot } from "../../../generated/agent-service-api";
import type { AgentRuntimeView } from "../../../generated/agent-runtime-validators";

export function validateContextSnapshotCommit(
  snapshot: AgentContextSnapshot,
  required: AgentRuntimeView["context"],
  committedRevision: bigint | null,
): bigint {
  const nextRevision = BigInt(snapshot.snapshot_revision);
  if (
    nextRevision < required.snapshot_revision ||
    (committedRevision != null && nextRevision < committedRevision)
  ) {
    throw new Error("返回的模型上下文低于当前 required revision");
  }
  if (
    nextRevision === required.snapshot_revision &&
    (snapshot.recipe_digest !== required.recipe_digest ||
      snapshot.context_revision !== required.context_revision)
  ) {
    throw new Error("返回的模型上下文与 Runtime context coordinate 不一致");
  }
  return nextRevision;
}
