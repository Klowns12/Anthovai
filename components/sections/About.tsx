import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'

export function About() {
  const t = useTranslations('about')

  return (
    <section className="py-24 lg:py-32 bg-bg-2 border-y border-white/[0.04]">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <FadeUp>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-16 lg:gap-24">
            {/* Left */}
            <div>
              <div className="flex items-center gap-3 mb-8">
                <span className="w-8 h-[1px] bg-gold" />
                <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('label')}
                </h2>
              </div>
              
              <h3 className="font-display text-[clamp(40px,5vw,64px)] leading-[1.05] tracking-[-0.01em] text-white">
                {t('headline')}
              </h3>
            </div>

            {/* Right */}
            <div className="space-y-6 pt-2">
              <p className="text-xl leading-relaxed text-white-60 font-medium">
                {t('p1')}
              </p>
              <p className="text-base leading-relaxed text-white-30">
                {t('p2')}
              </p>
              <p className="text-lg leading-relaxed text-gold italic font-display mt-8 border-l border-gold-border pl-6">
                {t('p3')}
              </p>
            </div>
          </div>
        </FaฤdeUp>
      </div>
    </section>
  )
}
