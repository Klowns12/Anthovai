import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { CTABand } from '@/components/sections/CTABand'

export default function SolutionsPage() {
  const t = useTranslations('solutions_page')

  const industries = ['finance', 'healthcare', 'manufacturing', 'government'] as const

  return (
    <>
      <main className="pt-32 pb-24">
        {/* Hero */}
        <section className="py-24 relative overflow-hidden">
          <div className="absolute inset-0 dot-grid opacity-20 pointer-events-none" />
          <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10">
            <FadeUp>
              <div className="flex items-center gap-3 mb-8">
                <span className="w-8 h-[1px] bg-gold" />
                <h1 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('hero_label')}
                </h1>
              </div>
              <h2 className="font-display text-[clamp(48px,6vw,80px)] leading-[1.05] tracking-[-0.01em] text-white max-w-4xl mb-8">
                {t('hero_headline')}
              </h2>
              <p className="text-xl text-white-60 leading-relaxed max-w-2xl">
                {t('hero_body')}
              </p>
            </FadeUp>
          </div>
        </section>

        {/* Industries */}
        <section className="py-24 bg-bg-2 border-y border-white/[0.04]">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp className="mb-16">
              <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30">
                {t('industries_label')}
              </h2>
            </FadeUp>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-x-8 gap-y-12">
              {industries.map((ind, i) => (
                <FadeUp key={ind} delay={i * 0.1}>
                  <div className="group p-8 border border-white/[0.06] bg-bg rounded-lg hover:border-gold-border hover:bg-gold/[0.02] transition-colors duration-300">
                    <h3 className="text-2xl font-medium text-white mb-4 flex items-center justify-between">
                      {t(`industries.${ind}.title`)}
                      <span className="text-gold opacity-0 group-hover:opacity-100 transition-opacity">
                        &rarr;
                      </span>
                    </h3>
                    <p className="text-white-60 leading-relaxed">
                      {t(`industries.${ind}.desc`)}
                    </p>
                  </div>
                </FadeUp>
              ))}
            </div>
          </div>
        </section>
      </main>
      <CTABand />
    </>
  )
}
