import { cn } from '@/lib/utils'

interface FieldProps extends React.InputHTMLAttributes<HTMLInputElement> {
  label: string
  /** Shown under the field, in gold-on-nothing rather than red-on-white: the
   * rest of this site has no red in it, and one error should not be the first
   * time a customer sees a colour they have not seen before. */
  error?: string | null
  hint?: string
}

export function Field({ label, error, hint, className, id, ...props }: FieldProps) {
  const fieldId = id ?? props.name

  return (
    <label className="block" htmlFor={fieldId}>
      <span className="block text-[11px] font-medium tracking-[0.2em] uppercase text-white-60 mb-3">
        {label}
      </span>
      <input
        id={fieldId}
        className={cn(
          'w-full bg-bg-2 border border-white/[0.08] rounded-md px-4 py-3',
          'text-white placeholder:text-white-30 font-sans',
          'transition-colors duration-200',
          'focus:outline-none focus:border-gold-border focus:bg-bg-3',
          error && 'border-gold-border',
          className
        )}
        aria-invalid={error ? true : undefined}
        aria-describedby={error ? `${fieldId}-error` : undefined}
        {...props}
      />
      {hint && !error && (
        <span className="block text-xs text-white-30 mt-2">{hint}</span>
      )}
      {error && (
        <span id={`${fieldId}-error`} className="block text-xs text-gold mt-2">
          {error}
        </span>
      )}
    </label>
  )
}
