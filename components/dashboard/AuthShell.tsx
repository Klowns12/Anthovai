import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'

interface AuthShellProps {
  label: string
  title: string
  intro: string
  children: React.ReactNode
  /** The other door: "already have an account?" and its mirror. */
  footer: React.ReactNode
}

/**
 * The frame around signing up and signing in.
 *
 * Narrow, centred, and quiet. The marketing pages above it are doing the
 * persuading; by the time someone is here they have decided, and the page's job
 * is to stay out of the way.
 */
export function AuthShell({ label, title, intro, children, footer }: AuthShellProps) {
  return (
    <main className="pt-32 pb-24 min-h-screen">
      <section className="py-16 relative overflow-hidden">
        <div className="absolute inset-0 dot-grid opacity-30 pointer-events-none" />
        <div className="mx-auto max-w-md px-6 relative z-10">
          <FadeUp>
            <h1 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-6">
              {label}
            </h1>
            <h2 className="font-display text-[clamp(32px,4vw,44px)] leading-[1.1] tracking-[-0.01em] text-white mb-4">
              {title}
            </h2>
            <p className="text-white-60 leading-relaxed mb-10">{intro}</p>

            {children}

            <p className="text-sm text-white-60 mt-8">{footer}</p>
          </FadeUp>
        </div>
      </section>
    </main>
  )
}

export function AuthLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <Link href={href} className="text-gold hover:text-gold-light transition-colors">
      {children}
    </Link>
  )
}
