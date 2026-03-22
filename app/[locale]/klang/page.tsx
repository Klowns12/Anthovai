import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'

export default function KlangPage() {
  const t = useTranslations('klang_page')

  const pillars = ['speed', 'readability', 'structure', 'ai'] as const
  const targets = ['native', 'llvm', 'wasm'] as const
  const ecosystem = ['klang', 'rumba', 'arkai', 'ocean'] as const

  return (
    <>
      <main className="pt-32 pb-24">
        {/* Hero */}
        <section className="py-24 relative overflow-hidden bg-bg-3 border-y border-white/[0.04]">
          <div className="absolute inset-0 dot-grid opacity-30 pointer-events-none" />
          <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10 text-center flex flex-col items-center">
            <FadeUp>
              <h1 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-8">
                {t('hero_label')}
              </h1>
              <h2 className="font-display text-[clamp(48px,6vw,80px)] leading-[1.05] tracking-[-0.01em] text-white max-w-4xl mb-8">
                {t('hero_headline')}
              </h2>
              <p className="text-xl text-white-60 leading-relaxed max-w-2xl mx-auto mb-12">
                {t('hero_body')}
              </p>
            </FadeUp>
          </div>
        </section>

        {/* Pillars Grid */}
        <section className="py-24">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp className="mb-16">
              <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30">
                {t('pillars_label')}
              </h2>
            </FadeUp>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-8">
              {pillars.map((p, i) => (
                <FadeUp key={p} delay={i * 0.1}>
                  <div className="p-8 border border-white/[0.06] bg-bg-2 rounded-lg">
                    <h3 className="text-xl font-medium text-gold mb-3">
                      {t(`pillars.${p}.title`)}
                    </h3>
                    <p className="text-white-60 leading-relaxed">
                      {t(`pillars.${p}.desc`)}
                    </p>
                  </div>
                </FadeUp>
              ))}
            </div>
          </div>
        </section>

        {/* Targets & Ecosystem */}
        <section className="py-24 bg-bg-2 border-y border-white/[0.04]">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-16 lg:gap-24">
              
              {/* Targets */}
              <div>
                <FadeUp className="mb-12">
                  <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30">
                    {t('targets_label')}
                  </h2>
                </FadeUp>
                <div className="space-y-6">
                  {targets.map((target, i) => (
                    <FadeUp key={target} delay={i * 0.1}>
                      <div className="pb-6 border-b border-white/[0.06]">
                        <h4 className="text-lg font-medium text-white mb-2">
                          {t(`targets.${target}.title`)}
                        </h4>
                        <p className="text-sm text-white-60">
                          {t(`targets.${target}.desc`)}
                        </p>
                      </div>
                    </FadeUp>
                  ))}
                </div>
              </div>

              {/* Ecosystem */}
              <div>
                <FadeUp className="mb-12">
                  <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30">
                    {t('ecosystem_label')}
                  </h2>
                </FadeUp>
                <div className="grid grid-cols-2 gap-4">
                  {ecosystem.map((eco, i) => (
                    <FadeUp key={eco} delay={i * 0.1}>
                      <div className="p-6 border border-white/[0.06] bg-bg rounded-lg h-full">
                        <h4 className="text-sm font-medium text-white mb-2">
                          {t(`ecosystem.${eco}.name`)}
                        </h4>
                        <p className="text-xs text-white-60">
                          {t(`ecosystem.${eco}.desc`)}
                        </p>
                      </div>
                    </FadeUp>
                  ))}
                </div>
              </div>

            </div>
          </div>
        </section>

        {/* Installation */}
        <section className="py-32 relative text-center">
          <div className="mx-auto max-w-3xl px-6 lg:px-8">
            <FadeUp>
              <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-8">
                {t('install_label')}
              </h2>
              <div className="bg-code-bg border border-white/[0.08] rounded-lg p-6 mb-8 inline-block shadow-2xl">
                <code className="text-sm font-mono text-[#B5CEA8]">
                  {t('install_command')}
                </code>
              </div>
              <div>
                <Link 
                  href="/klang/docs" 
                  className="text-sm tracking-[0.15em] uppercase text-white-60 hover:text-white transition-colors border-b border-white/20 hover:border-white pb-1"
                >
                  {t('docs_cta')} &rarr;
                </Link>
              </div>
            </FadeUp>
          </div>
        </section>
      </main>
    </>
  )
}
