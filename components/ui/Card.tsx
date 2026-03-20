import { cn } from '@/lib/utils'

interface CardProps {
  children: React.ReactNode
  className?: string
  hover?: boolean
}

export function Card({ children, className, hover = true }: CardProps) {
  return (
    <div
      className={cn(
        'relative bg-bg-2 border border-white/[0.06] rounded-lg p-8 overflow-hidden',
        hover && 'transition-all duration-300 hover:border-gold-border hover:shadow-gold',
        className
      )}
    >
      {children}
    </div>
  )
}
