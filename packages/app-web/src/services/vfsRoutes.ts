function encoded(value: string): string {
  return encodeURIComponent(value);
}

function withQuery(path: string, params: URLSearchParams): string {
  const qs = params.toString();
  return qs ? `${path}?${qs}` : path;
}

export const vfsRoutes = {
  spaces(params: URLSearchParams): string {
    return withQuery("/vfs", params);
  },
  entries(spaceId: string, params: URLSearchParams): string {
    return withQuery(`/vfs/${encoded(spaceId)}/entries`, params);
  },
  surfaces: {
    resolve: "/vfs-surfaces/resolve",
    byRef(surfaceRef: string): string {
      return `/vfs-surfaces/${encoded(surfaceRef)}`;
    },
    entries(surfaceRef: string, mountId: string, params: URLSearchParams): string {
      return withQuery(
        `/vfs-surfaces/${encoded(surfaceRef)}/mounts/${encoded(mountId)}/entries`,
        params,
      );
    },
    readFile: "/vfs-surfaces/read-file",
    writeFile: "/vfs-surfaces/write-file",
    createFile: "/vfs-surfaces/create-file",
    deleteFile: "/vfs-surfaces/delete-file",
    renameFile: "/vfs-surfaces/rename-file",
    statFile: "/vfs-surfaces/stat-file",
    applyPatch: "/vfs-surfaces/apply-patch",
  },
  filePicker: {
    list(params: URLSearchParams): string {
      return withQuery("/file-picker", params);
    },
    batchRead: "/file-picker/batch-read",
  },
} as const;
