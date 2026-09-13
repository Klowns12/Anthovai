import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '../animations/FadeUp'
import { ArrowRight } from 'lucide-react'

/**
 * The platform, installed inside the customer's own network — on the home page.
 *
 * A teaser, on the same terms as `PlatformSection` above it: a headline, three
 * lines and a way in, with the whole argument left to `/platform/private-ai`.
 *
 * It follows the platform section rather than replacing it because the two are
 * the same product for two different buyers. The one above signs up alone
 * this afternoon; this one has a compliance officer, a server room, and
 * documents that were never going to be uploaded anywhere.
 */
export function PrivateAiSection() {
  const t = useTranslations('private_ai')
  const points = t.raw('points') as string[]

  return (
    <section className="py-24 lg:py-32 border-b border-white/[0.04] bg-bg-2/60">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <FadeUp>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-16 lg:gap-24 items-start">
            {/* Left: the argument, in order */}
            <div className="lg:order-2">
              <div className="flex items-center gap-3 mb-8">
                <span className="w-8 h-[1px] bg-gold" />
                <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('label')}
                </h2>
              </div>

              <h3 className="font-display text-[clamp(34px,4.5vw,56px)] leading-[1.08] tracking-[-0.01em] text-white [text-wrap:balance]">
                {t('headline')}
              </h3>

              <div className="flex flex-wrap items-center gap-4 mt-10">
                <Link
                  href="/platform/private-ai"
                  className="group inline-flex items-center gap-2 bg-gold text-bg font-medium text-sm tracking-wide px-7 py-3.5 rounded-md hover:bg-gold-light transition-colors"
                >
                  {t('cta')}
                  <ArrowRight
                    className="w-4 h-4 transition-transform group-hover:translate-x-0.5"
                    strokeWidth={2}
                  />
                </Link>
                <Link
                  href="/contact"
                  className="text-sm tracking-wide px-7 py-3.5 rounded-md border border-white/[0.12] text-white-60 hover:text-white hover:border-white/[0.24] transition-colors"
                >
                  {t('cta_demo')}
                </Link>
              </div>
            </div>

            {/* Right: what it is, and the three things that make it different */}
            <div className="pt-2 lg:order-1">
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
