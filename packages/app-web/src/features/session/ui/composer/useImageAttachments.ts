import { useCallback, useRef, useState } from "react";

export const MAX_IMAGE_SIZE_BYTES = 5 * 1024 * 1024; // 5 MB
export const MAX_IMAGE_COUNT = 6;

export interface ImageAttachment {
  id: string;
  file: File;
  dataUrl: string;
  previewUrl: string;
}

export interface UseImageAttachmentsResult {
  attachments: ImageAttachment[];
  error: string | null;
  addFromFiles: (files: FileList | File[]) => void;
  addFromClipboard: (items: DataTransferItemList) => void;
  addFromDrop: (items: DataTransfer) => void;
  removeAttachment: (id: string) => void;
  clearAll: () => void;
  clearError: () => void;
}

let nextId = 0;
function genId(): string {
  return `img-${Date.now()}-${nextId++}`;
}

function isImageFile(file: File): boolean {
  return file.type.startsWith("image/");
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(new Error(`读取文件失败: ${file.name}`));
    reader.readAsDataURL(file);
  });
}

export function useImageAttachments(): UseImageAttachmentsResult {
  const [attachments, setAttachments] = useState<ImageAttachment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const previewUrlsRef = useRef<string[]>([]);

  const processFiles = useCallback(async (files: File[]) => {
    const imageFiles = files.filter(isImageFile);
    if (imageFiles.length === 0) return;

    setError(null);

    setAttachments((prev) => {
      const remaining = MAX_IMAGE_COUNT - prev.length;
      if (remaining <= 0) {
        setError(`最多添加 ${MAX_IMAGE_COUNT} 张图片。`);
        return prev;
      }
      if (imageFiles.length > remaining) {
        setError(`最多添加 ${MAX_IMAGE_COUNT} 张图片，已自动截断。`);
      }
      return prev; // real processing is async, handled below
    });

    const currentCount = attachments.length; // snapshot for validation
    const remaining = MAX_IMAGE_COUNT - currentCount;
    if (remaining <= 0) {
      setError(`最多添加 ${MAX_IMAGE_COUNT} 张图片。`);
      return;
    }

    const toProcess = imageFiles.slice(0, remaining);
    if (imageFiles.length > remaining) {
      setError(`最多添加 ${MAX_IMAGE_COUNT} 张图片，已自动截断。`);
    }

    const oversized = toProcess.filter((f) => f.size > MAX_IMAGE_SIZE_BYTES);
    if (oversized.length > 0) {
      setError(`图片 ${oversized.map((f) => f.name).join(", ")} 超过 5MB 限制，已跳过。`);
    }

    const validFiles = toProcess.filter((f) => f.size <= MAX_IMAGE_SIZE_BYTES);
    if (validFiles.length === 0) return;

    const newAttachments: ImageAttachment[] = [];
    for (const file of validFiles) {
      try {
        const dataUrl = await readFileAsDataUrl(file);
        const previewUrl = URL.createObjectURL(file);
        previewUrlsRef.current.push(previewUrl);
        newAttachments.push({
          id: genId(),
          file,
          dataUrl,
          previewUrl,
        });
      } catch {
        setError(`读取 ${file.name} 失败。`);
      }
    }

    if (newAttachments.length > 0) {
      setAttachments((prev) => [...prev, ...newAttachments]);
    }
  }, [attachments.length]);

  const addFromFiles = useCallback(
    (files: FileList | File[]) => {
      void processFiles(Array.from(files));
    },
    [processFiles],
  );

  const addFromClipboard = useCallback(
    (items: DataTransferItemList) => {
      const files: File[] = [];
      for (let i = 0; i < items.length; i++) {
        const item = items[i];
        if (item?.kind === "file" && item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) files.push(file);
        }
      }
      if (files.length > 0) {
        void processFiles(files);
      }
    },
    [processFiles],
  );

  const addFromDrop = useCallback(
    (transfer: DataTransfer) => {
      const files: File[] = [];
      for (let i = 0; i < transfer.files.length; i++) {
        const file = transfer.files[i];
        if (file && isImageFile(file)) {
          files.push(file);
        }
      }
      if (files.length > 0) {
        void processFiles(files);
      }
    },
    [processFiles],
  );

  const removeAttachment = useCallback((id: string) => {
    setAttachments((prev) => {
      const item = prev.find((a) => a.id === id);
      if (item) {
        URL.revokeObjectURL(item.previewUrl);
        previewUrlsRef.current = previewUrlsRef.current.filter((u) => u !== item.previewUrl);
      }
      return prev.filter((a) => a.id !== id);
    });
  }, []);

  const clearAll = useCallback(() => {
    setAttachments((prev) => {
      for (const a of prev) URL.revokeObjectURL(a.previewUrl);
      previewUrlsRef.current = [];
      return [];
    });
    setError(null);
  }, []);

  const clearError = useCallback(() => setError(null), []);

  return {
    attachments,
    error,
    addFromFiles,
    addFromClipboard,
    addFromDrop,
    removeAttachment,
    clearAll,
    clearError,
  };
}
