export interface RuntimeContextRequestToken {
  target_key: string;
  generation: number;
}

export function shouldApplyRuntimeContextResponse(
  mounted: boolean,
  latest: RuntimeContextRequestToken | null,
  request: RuntimeContextRequestToken,
): boolean {
  return mounted
    && latest?.target_key === request.target_key
    && latest.generation === request.generation;
}
