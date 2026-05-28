import { useEffect } from 'react'

import { Notice, type NoticeTone } from './Notice'

export interface DismissibleNoticeData {
  tone: NoticeTone
  message: string
}

export interface DismissibleNoticeProps {
  notice: DismissibleNoticeData | null
  onDismiss: () => void
  /** 0 表示不自动消失；默认 4000ms。 */
  autoHideMs?: number
}

export function DismissibleNotice({
  notice,
  onDismiss,
  autoHideMs = 4000,
}: DismissibleNoticeProps) {
  useEffect(() => {
    if (!notice || autoHideMs <= 0) return
    const timer = setTimeout(onDismiss, autoHideMs)
    return () => clearTimeout(timer)
  }, [notice, autoHideMs, onDismiss])

  if (!notice) return null

  return (
    <Notice
      tone={notice.tone}
      role={notice.tone === 'danger' ? 'alert' : 'status'}
      className="flex items-center justify-between text-xs"
    >
      <p>{notice.message}</p>
      <button
        type="button"
        onClick={onDismiss}
        className="ml-2 text-xs opacity-70 transition-opacity hover:opacity-100"
        aria-label="关闭提示"
      >
        ×
      </button>
    </Notice>
  )
}
