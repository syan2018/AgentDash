import type { ComponentPropsWithoutRef, HTMLAttributes, ReactNode } from 'react'

import { cn } from '../utils/cn'

type SectionCardProps = ComponentPropsWithoutRef<'section'> & {
  as?: 'section'
  children: ReactNode
}

type ArticleCardProps = ComponentPropsWithoutRef<'article'> & {
  as: 'article'
  children: ReactNode
}

type DivCardProps = ComponentPropsWithoutRef<'div'> & {
  as: 'div'
  children: ReactNode
}

type FormCardProps = ComponentPropsWithoutRef<'form'> & {
  as: 'form'
  children: ReactNode
}

export type CardProps = SectionCardProps | ArticleCardProps | DivCardProps | FormCardProps

export interface CardHeaderProps extends HTMLAttributes<HTMLDivElement> {
  actions?: ReactNode
  children: ReactNode
}

export function Card(props: CardProps) {
  const className = cn('rounded-[8px] border border-border bg-card p-4', props.className)

  if (props.as === 'form') {
    const { as: _as, children, className: _className, ...formProps } = props
    return (
      <form className={className} {...formProps}>
        {children}
      </form>
    )
  }

  if (props.as === 'article') {
    const { as: _as, children, className: _className, ...articleProps } = props
    return (
      <article className={className} {...articleProps}>
        {children}
      </article>
    )
  }

  if (props.as === 'div') {
    const { as: _as, children, className: _className, ...divProps } = props
    return (
      <div className={className} {...divProps}>
        {children}
      </div>
    )
  }

  const { as: _as, children, className: _className, ...sectionProps } = props
  return (
    <section className={className} {...sectionProps}>
      {children}
    </section>
  )
}

export function CardHeader({ actions, children, className, ...props }: CardHeaderProps) {
  return (
    <div className={cn('mb-4 flex items-center justify-between gap-3', className)} {...props}>
      <div className="min-w-0">{children}</div>
      {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
    </div>
  )
}
