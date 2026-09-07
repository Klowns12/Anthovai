import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '../animations/FadeUp'
import { ArrowRight } from 'lucide-react'

/**
 * The platform, on the home page.
 *
 * A teaser and not a second copy of `/platform`: three lines and a way in. A
 * home section that says everything leaves the page it links to with nothing
 * to add, and a visitor who has read both has read the same thing twice.
 *
 * The two doors are deliberate. "See how it works" is for the visitor still
 * deciding; "Create an account" is for the one who has already decided and
 * should not have to pass through a marketing page to act on it.
 */
export function PlatformSection() {
  const t = useTranslations('platform')
  const points = t.raw('points') as string[]

  return (
    <section className="py-24 lg:py-32 border-y border-white/[0.04] relative overflow-hidden">
      <div className="absolute inset-0 dot-grid opacity-[0.12] pointer-events-none" />

      <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10">
        <FadeUp>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-16 lg:gap-24 items-start">
            {/* Left */}
            <div>
              <div className="flex items-center gap-3 mb-8">
                <span className="w-8 h-[1px] bg-gold" />
                <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('label')}
                </h2>
              </div>

              <h3 className="font-display text-[clamp(34px,4.5vw,56px)] leading-[1.08] tracking-[-0.01em] text-white">
                {t('headline')}
              </h3>

              <div className="flex flex-wrap items-center gap-4 mt-10">
                <Link
                  href="/platform"
                  className="group inline-flex items-center gap-2 bg-gold text-bg font-medium text-sm tracking-wide px-7 py-3.5 rounded-md hover:bg-gold-light transition-colors"
                >
                  {t('cta')}
                  <ArrowRight
                    className="w-4 h-4 transition-transform group-hover:translate-x-0.5"
                    strokeWidth={2}
                  />
                </Link>
                <Link
                  href="/signup"
                  className="text-sm tracking-wide px-7 py-3.5 rounded-md border border-white/[0.12] text-white-60 hover:text-white hover:border-white/[0.24] transition-colors"
                >
                  {t('cta_signup')}
                </Link>
              </div>
            </div>

            {/* Right */}
            <div className="pt-2">
              <p className="text-lg leading-relaxed text-white-60">{t('sub')}</p>

              <ul className="mt-10 space-y-5">
                {points.map((point, index) => (
                  <li key={point} className="flex gap-5">
                    <span className="text-[11px] font-mono tracking-[0.2em] text-gold pt-1.5 shrink-0">
                      {String(index + 1).padStart(2, '0')}
                    </span>
                    <span className="text-white-60 leading-relaxed border-l border-white/[0.06] pl-5">
                      {point}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </FadeUp>
      </div>
    </section>
  )
}
