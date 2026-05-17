/**
 * App-web 专用包装：在 views 层通用 DirectoryBrowserDialog 上绑定 backend API。
 */

import {
  DirectoryBrowserDialog as BaseDialog,
} from '@agentdash/views/directory-browser'
import type { BrowseDirectoryResult } from '@agentdash/views/directory-browser'
import { useCallback } from 'react'
import { browseDirectory } from '../../services/browseDirectory'

interface DirectoryBrowserDialogProps {
  open: boolean
  backendId: string
  initialPath?: string
  onSelect: (path: string) => void
  onClose: () => void
}

export function DirectoryBrowserDialog({
  open,
  backendId,
  initialPath,
  onSelect,
  onClose,
}: DirectoryBrowserDialogProps) {
  const handleBrowse = useCallback(
    async (path?: string): Promise<BrowseDirectoryResult> => {
      return browseDirectory(backendId, path)
    },
    [backendId],
  )

  return (
    <BaseDialog
      open={open}
      initialPath={initialPath}
      onBrowse={handleBrowse}
      onSelect={onSelect}
      onClose={onClose}
    />
  )
}
