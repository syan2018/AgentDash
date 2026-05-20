import type { ButtonHTMLAttributes } from 'react'

import { Button, type ButtonSize, type ButtonVariant } from './Button'

export interface CreateButtonProps
  extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  /** 实体英文名（Story / Skill / Workflow ...），渲染为 "+ {entity}" */
  entity: string
  variant?: ButtonVariant
  size?: ButtonSize
}

export function CreateButton({
  entity,
  variant = 'primary',
  size = 'sm',
  ...rest
}: CreateButtonProps) {
  return (
    <Button variant={variant} size={size} {...rest}>
      <span aria-hidden className="mr-1">+</span>
      {entity}
    </Button>
  )
}
