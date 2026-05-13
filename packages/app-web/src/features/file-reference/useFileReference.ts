import { useCallback, useRef, useState } from "react";
import type { FileEntry } from "../../services/filePicker";
import { listFiles } from "../../services/filePicker";

export interface FileReference {
  relPath: string;
  size: number;
}

const MAX_REFERENCES = 10;

export function useFileReference(workspaceId?: string | null) {
  const [references, setReferences] = useState<FileReference[]>([]);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerQuery, setPickerQuery] = useState("");
  const [pickerFiles, setPickerFiles] = useState<FileEntry[]>([]);
  const [pickerLoading, setPickerLoading] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const fetchFiles = useCallback(async (query: string) => {
    if (!workspaceId) {
      setPickerError("当前会话没有可用的工作空间，暂不支持 @ 文件引用");
      setPickerFiles([]);
      setPickerLoading(false);
      return;
    }
    setPickerLoading(true);
    setPickerError(null);
    try {
      const result = await listFiles(workspaceId, query || undefined);
      setPickerFiles(result.files.filter((f) => f.isText));
    } catch (e) {
      setPickerError(e instanceof Error ? e.message : "加载文件列表失败");
      setPickerFiles([]);
    } finally {
      setPickerLoading(false);
    }
  }, [workspaceId]);

  const openPicker = useCallback((initialQuery = "") => {
    if (!workspaceId) {
      setPickerOpen(false);
      setPickerFiles([]);
      setPickerLoading(false);
      setPickerError("当前会话没有可用的工作空间，暂不支持 @ 文件引用");
      return;
    }
    setPickerOpen(true);
    setPickerQuery(initialQuery);
    setSelectedIndex(0);
    setPickerError(null);
    void fetchFiles(initialQuery);
  }, [fetchFiles, workspaceId]);

  const closePicker = useCallback(() => {
    setPickerOpen(false);
    setPickerQuery("");
    setPickerFiles([]);
    setPickerError(null);
    setSelectedIndex(0);
  }, []);

  const updateQuery = useCallback((query: string) => {
    setPickerQuery(query);
    setSelectedIndex(0);
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      void fetchFiles(query);
    }, 200);
  }, [fetchFiles]);

  const addReference = useCallback(
    (file: FileEntry) => {
      setReferences((prev) => {
        if (prev.length >= MAX_REFERENCES) return prev;
        if (prev.some((r) => r.relPath === file.relPath)) return prev;
        return [...prev, { relPath: file.relPath, size: file.size }];
      });
      closePicker();
    },
    [closePicker],
  );

  const removeReference = useCallback((relPath: string) => {
    setReferences((prev) => prev.filter((r) => r.relPath !== relPath));
  }, []);

  const clearReferences = useCallback(() => {
    setReferences([]);
  }, []);

  const moveSelection = useCallback(
    (delta: number) => {
      setSelectedIndex((prev) => {
        const len = pickerFiles.length;
        if (len === 0) return 0;
        return (prev + delta + len) % len;
      });
    },
    [pickerFiles.length],
  );

  const confirmSelection = useCallback(() => {
    if (pickerFiles.length > 0 && selectedIndex < pickerFiles.length) {
      addReference(pickerFiles[selectedIndex]);
    }
  }, [pickerFiles, selectedIndex, addReference]);

  return {
    references,
    pickerOpen,
    pickerQuery,
    pickerFiles,
    pickerLoading,
    pickerError,
    selectedIndex,
    openPicker,
    closePicker,
    updateQuery,
    addReference,
    removeReference,
    clearReferences,
    moveSelection,
    confirmSelection,
    canAddMore: references.length < MAX_REFERENCES,
    hasWorkspaceContext: Boolean(workspaceId),
  };
}
