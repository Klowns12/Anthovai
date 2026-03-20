import { cn } from '@/lib/utils'

interface BadgeProps {
  children: React.ReactNode
  className?: string
}

export function Badge({ children, className }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center px-3 py-1 text-[10px] font-medium tracking-[0.2em] uppercase',
        'border border-gold-border text-gold bg-gold-dim rounded-sm',
        className
      )}
    >
      {children}
    </span>
  )
}
