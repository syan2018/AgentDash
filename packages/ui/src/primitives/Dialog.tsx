import { useEffect, useId, useRef, type KeyboardEvent, type ReactNode } from 'react'

import { Button } from './Button'
import { TextInput } from './TextInput'
import { cn } from '../utils/cn'

export type ConfirmDialogTone = 'default' | 'danger'

export interface DialogFrameProps {
  open: boolean
  title: string
  description?: string
  children?: ReactNode
  footer: ReactNode
  onClose: () => void
}

export interface ConfirmDialogProps {
  open: boolean
  title: string
  description: string
  confirmLabel: string
  cancelLabel?: string
  tone?: ConfirmDialogTone
  disabled?: boolean
  isConfirming?: boolean
  onClose: () => void
  onConfirm: () => void
}

export interface PromptDialogProps {
  open: boolean
  title: string
  label: string
  value: string
  confirmLabel: string
  description?: string
  placeholder?: string
  cancelLabel?: string
  disabled?: boolean
  isConfirming?: boolean
  error?: string | null
  onValueChange: (value: string) => void
  onClose: () => void
  onConfirm: () => void
}

export function DialogFrame({
  open,
  title,
  description,
  children,
  footer,
  onClose,
}: DialogFrameProps) {
  const titleId = useId()
  const descriptionId = useId()

  useEffect(() => {
    if (!open) return
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
      }
    }
    document.addEventListener('keydown', handleKeyDown)
    return () => document.removeEventListener('keydown', handleKeyDown)
  }, [open, onClose])

  if (!open) return null

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4" onClick={onClose}>
        <section
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          aria-describedby={description ? descriptionId : undefined}
          className="w-full max-w-lg rounded-[12px] border border-border bg-background shadow-2xl"
          onClick={(event) => event.stopPropagation()}
        >
          <header className="border-b border-border px-5 py-4">
            <h4 id={titleId} className="text-base font-semibold text-foreground">
              {title}
            </h4>
            {description && (
              <p id={descriptionId} className="mt-1.5 text-sm leading-6 text-muted-foreground">
                {description}
              </p>
            )}
          </header>
          {children && <div className="space-y-3 p-5">{children}</div>}
          <footer className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            {footer}
          </footer>
        </section>
      </div>
    </>
  )
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  cancelLabel = '取消',
  tone = 'default',
  disabled = false,
  isConfirming = false,
  onClose,
  onConfirm,
}: ConfirmDialogProps) {
  const confirmVariant = tone === 'danger' ? 'danger' : 'primary'

  return (
    <DialogFrame
      open={open}
      title={title}
      description={description}
      onClose={onClose}
      footer={(
        <>
          <Button variant="secondary" onClick={onClose} disabled={isConfirming}>
            {cancelLabel}
          </Button>
          <Button
            variant={confirmVariant}
            onClick={onConfirm}
            disabled={disabled || isConfirming}
          >
            {isConfirming ? '处理中...' : confirmLabel}
          </Button>
        </>
      )}
    />
  )
}

export function PromptDialog({
  open,
  title,
  label,
  value,
  confirmLabel,
  description,
  placeholder,
  cancelLabel = '取消',
  disabled = false,
  isConfirming = false,
  error,
  onValueChange,
  onClose,
  onConfirm,
}: PromptDialogProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const labelId = useId()
  const canConfirm = !disabled && !isConfirming

  useEffect(() => {
    if (!open) return
    requestAnimationFrame(() => inputRef.current?.focus())
  }, [open])

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault()
      if (canConfirm) onConfirm()
    }
  }

  return (
    <DialogFrame
      open={open}
      title={title}
      description={description}
      onClose={onClose}
      footer={(
        <>
          <Button variant="secondary" onClick={onClose} disabled={isConfirming}>
            {cancelLabel}
          </Button>
          <Button variant="primary" onClick={onConfirm} disabled={!canConfirm}>
            {isConfirming ? '处理中...' : confirmLabel}
          </Button>
        </>
      )}
    >
      <label className="block space-y-1.5" htmlFor={labelId}>
        <span className="agentdash-form-label">{label}</span>
        <TextInput
          ref={inputRef}
          id={labelId}
          value={value}
          placeholder={placeholder}
          disabled={isConfirming}
          onChange={(event) => onValueChange(event.target.value)}
          onKeyDown={handleKeyDown}
          className={cn(error ? 'border-destructive/40 focus:border-destructive/40 focus:ring-destructive/30' : null)}
        />
      </label>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </DialogFrame>
  )
}
