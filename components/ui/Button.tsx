import { cn } from '@/lib/utils'
import { Link } from '@/i18n/navigation'

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'sm' | 'md' | 'lg'
  href?: string
  children: React.ReactNode
}

export function Button({
  variant = 'primary',
  size = 'md',
  href,
  children,
  className,
  ...props
}: ButtonProps) {
  const baseStyles =
    'inline-flex items-center justify-center font-medium transition-all duration-300 ease-out focus-visible:outline-2 focus-visible:outline-gold focus-visible:outline-offset-2'

  const variants = {
    primary:
      'bg-gold text-bg hover:bg-gold-light active:scale-[0.98]',
    secondary:
      'border border-gold-border text-gold hover:bg-gold-dim active:scale-[0.98]',
    ghost:
      'text-white-60 hover:text-white transition-colors',
  }

  const sizes = {
    sm: 'px-4 py-2 text-xs tracking-wider uppercase',
    md: 'px-6 py-3 text-sm tracking-wider uppercase',
    lg: 'px-8 py-4 text-sm tracking-wider uppercase',
  }

  const classes = cn(baseStyles, variants[variant], sizes[size], className)

  if (href) {
    return (
      <Link href={href} className={classes}>
        {children}
      </Link>
    )
  }

  return (
    <button className={classes} {...props}>
      {children}
    </button>
  )
}
