import { useCallback, useEffect, useMemo, useState } from "react";

import { fetchLibraryAssets } from "../../../services/sharedLibrary";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import type { LibraryAssetDto, LibraryAssetType } from "../../../types";

export interface UseLibraryPublishedAssetsResult {
  /** key → asset；仅包含当前用户作为 author 已发布的资产。currentUserId 为空时为空 Map。 */
  publishedByKey: Map<string, LibraryAssetDto>;
  /** 触发一次 refetch（PublishLibraryAssetDialog 完成后调用）。 */
  reloadPublished: () => void;
}

/**
 * 拉取"当前用户已发布到资源市场"的某一类资产，按 asset.key 建索引，
 * 用于在各 CategoryPanel 卡片上展示"已发布 vX.Y.Z"徽章。
 *
 * 替换原本散落在 5 个 panel 里的 useState + useEffect + useMemo + reloadTick 重复块。
 */
export function useLibraryPublishedAssets(
  assetType: LibraryAssetType,
): UseLibraryPublishedAssetsResult {
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);

  const [publishedAssets, setPublishedAssets] = useState<LibraryAssetDto[]>([]);
  const [reloadTick, setReloadTick] = useState(0);

  useEffect(() => {
    if (!currentUserId) return;
    let cancelled = false;
    fetchLibraryAssets({ asset_type: assetType, owner_id: currentUserId })
      .then((list) => {
        if (!cancelled) setPublishedAssets(list);
      })
      .catch(() => {
        if (!cancelled) setPublishedAssets([]);
      });
    return () => {
      cancelled = true;
    };
  }, [assetType, currentUserId, reloadTick]);

  // currentUserId 为空时直接返回空 Map：旧用户残留的 publishedAssets state 不需要清空，
  // 因为这里就直接屏蔽了。下次切到新用户会重新 fetch 覆盖。
  const publishedByKey = useMemo(() => {
    if (!currentUserId) return new Map<string, LibraryAssetDto>();
    const map = new Map<string, LibraryAssetDto>();
    for (const asset of publishedAssets) {
      if (asset.source === "user_authored") map.set(asset.key, asset);
    }
    return map;
  }, [currentUserId, publishedAssets]);

  const reloadPublished = useCallback(() => {
    setReloadTick((tick) => tick + 1);
  }, []);

  return { publishedByKey, reloadPublished };
}
