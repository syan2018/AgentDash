import { useCallback, useEffect, useRef, useState } from "react";
import {
  listAddressEntries,
  listVfss,
  type VfsEntry,
  type VfsDescriptor,
} from "../../../services/vfs";

export interface UseVfsPickerOptions {
  spaceId: string;
  workspaceId: string | null | undefined;
  /** 额外的触发依赖（如 storyId 变化时重新发现） */
  resetKey?: string;
}

export interface UseVfsPickerResult {
  space: VfsDescriptor | null;
  spaceError: string | null;
  isAvailable: boolean;

  pickerOpen: boolean;
  pickerQuery: string;
  pickerEntries: VfsEntry[];
  pickerLoading: boolean;
  pickerError: string | null;
  selectedIndex: number;

  openPicker: () => void;
  closePicker: () => void;
  updatePickerQuery: (query: string) => void;
  moveSelection: (delta: number) => void;
  confirmSelection: () => VfsEntry | null;
}

const DEBOUNCE_MS = 200;

export function useVfsPicker(
  options: UseVfsPickerOptions,
): UseVfsPickerResult {
  const { spaceId, workspaceId, resetKey } = options;

  const [space, setSpace] = useState<VfsDescriptor | null>(null);
  const [spaceError, setSpaceError] = useState<string | null>(null);

  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerQuery, setPickerQuery] = useState("");
  const [pickerEntries, setPickerEntries] = useState<VfsEntry[]>([]);
  const [pickerLoading, setPickerLoading] = useState(false);
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function discover() {
      if (!workspaceId) {
        setSpace(null);
        setSpaceError("当前 Project 尚未配置默认工作空间，暂时无法快捷选择文件。");
        return;
      }

      try {
        setSpaceError(null);
        const result = await listVfss({ workspaceId });
        if (cancelled) return;
        const found = result.spaces.find((s) => s.id === spaceId) ?? null;
        setSpace(found);
        if (!found) {
          setSpaceError(`当前环境未暴露 ${spaceId} 寻址能力。`);
        }
      } catch (err) {
        if (cancelled) return;
        setSpace(null);
        setSpaceError(err instanceof Error ? err.message : "加载寻址空间失败");
      }
    }

    void discover();
    return () => {
      cancelled = true;
    };
  }, [spaceId, workspaceId, resetKey]);

  useEffect(() => {
    setPickerOpen(false);
    setPickerQuery("");
    setPickerEntries([]);
    setPickerError(null);
    setSelectedIndex(0);
  }, [resetKey]);

  const fetchEntries = useCallback(
    async (query: string) => {
      if (!space || !workspaceId) {
        setPickerEntries([]);
        setPickerError("没有可用的寻址空间");
        return;
      }
      setPickerLoading(true);
      setPickerError(null);
      try {
        const result = await listAddressEntries(space.id, {
          workspaceId,
          query: query || undefined,
        });
        setPickerEntries(result.entries);
        setSelectedIndex(0);
      } catch (err) {
        setPickerEntries([]);
        setPickerError(err instanceof Error ? err.message : "加载条目列表失败");
      } finally {
        setPickerLoading(false);
      }
    },
    [space, workspaceId],
  );

  const openPicker = useCallback(() => {
    setPickerOpen(true);
    setPickerQuery("");
    setSelectedIndex(0);
    setPickerError(null);
    void fetchEntries("");
  }, [fetchEntries]);

  const closePicker = useCallback(() => {
    setPickerOpen(false);
    setPickerQuery("");
    setPickerEntries([]);
    setPickerError(null);
    setSelectedIndex(0);
  }, []);

  const updatePickerQuery = useCallback(
    (query: string) => {
      setPickerQuery(query);
      setSelectedIndex(0);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void fetchEntries(query);
      }, DEBOUNCE_MS);
    },
    [fetchEntries],
  );

  const moveSelection = useCallback(
    (delta: number) => {
      setSelectedIndex((prev) => {
        const len = pickerEntries.length;
        if (len === 0) return 0;
        return (prev + delta + len) % len;
      });
    },
    [pickerEntries.length],
  );

  const confirmSelection = useCallback((): VfsEntry | null => {
    if (pickerEntries.length > 0 && selectedIndex < pickerEntries.length) {
      return pickerEntries[selectedIndex];
    }
    return null;
  }, [pickerEntries, selectedIndex]);

  return {
    space,
    spaceError,
    isAvailable: space !== null,

    pickerOpen,
    pickerQuery,
    pickerEntries,
    pickerLoading,
    pickerError,
    selectedIndex,

    openPicker,
    closePicker,
    updatePickerQuery,
    moveSelection,
    confirmSelection,
  };
}
