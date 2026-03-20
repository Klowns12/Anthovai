import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { CTABand } from '@/components/sections/CTABand'

export default function AboutPage() {
  const t = useTranslations('about_page')

  const values = ['precision', 'ownership', 'transparency', 'ambition'] as const

  return (
    <>
      <main className="pt-32 pb-24">
        {/* Hero */}
        <section className="py-24 relative overflow-hidden">
          <div className="absolute inset-0 bg-radial-gradient from-gold/[0.03] to-transparent pointer-events-none" />
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

        {/* Mission */}
        <section className="py-24 bg-bg-2 border-y border-white/[0.04]">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp>
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-16 lg:gap-24">
                <div>
                  <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-6">
                    {t('mission_label')}
                  </h2>
                  <h3 className="font-display text-4xl leading-[1.1] text-white mb-8">
                    {t('mission_headline')}
                  </h3>
                </div>
                <div>
                  <p className="text-xl leading-relaxed text-white-60 font-medium pt-2">
                    {t('mission_body')}
                  </p>
                </div>
              </div>
            </FadeUp>
          </div>
        </section>

        {/* Values */}
        <section className="py-24 lg:py-32">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp className="mb-16">
              <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 text-center">
                {t('values_label')}
              </h2>
            </FadeUp>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-8 lg:gap-12">
              {values.map((v, i) => (
                <FadeUp key={v} delay={i * 0.1}>
                  <div className="bg-bg-2 border border-white/[0.06] p-10 rounded-lg h-full">
                    <div className="font-mono text-sm text-gold mb-6">0{i + 1} //</div>
                    <h3 className="text-2xl font-medium text-white mb-4">
                      {t(`values.${v}.title`)}
                    </h3>
                    <p className="text-white-60 leading-relaxed">
                      {t(`values.${v}.desc`)}
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
