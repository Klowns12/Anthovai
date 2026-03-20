import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { Button } from '@/components/ui/Button'
import { Earth, Zap, Users, Code2 } from 'lucide-react'

export default function CareersPage() {
  const t = useTranslations('careers_page')

  const whyItems = [
    { key: 'impact', icon: Zap },
    { key: 'growth', icon: Code2 },
    { key: 'culture', icon: Users },
    { key: 'remote', icon: Earth },
  ] as const

  return (
    <main className="pt-32 pb-24">
      {/* Hero */}
      <section className="py-24 relative overflow-hidden">
        <div className="absolute inset-0 bg-radial-gradient from-gold/[0.05] to-transparent opacity-60 pointer-events-none" />
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

      {/* Why Anthovai */}
      <section className="py-24 bg-bg-2 border-y border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp className="mb-16 text-center">
            <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30">
              {t('why_label')}
            </h2>
          </FadeUp>

          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-8">
            {whyItems.map((item, i) => {
              const Icon = item.icon
              return (
                <FadeUp key={item.key} delay={i * 0.1}>
                  <div className="p-8 border border-white/[0.06] bg-bg rounded-lg h-full text-center flex flex-col items-center">
                    <div className="w-12 h-12 rounded-full border border-white/[0.08] bg-bg-2 flex items-center justify-center mb-6">
                      <Icon size={20} className="text-gold" />
                    </div>
                    <h3 className="text-lg font-medium text-white mb-3">
                      {t(`why_items.${item.key}.title`)}
                    </h3>
                    <p className="text-sm text-white-60 leading-relaxed">
                      {t(`why_items.${item.key}.desc`)}
                    </p>
                  </div>
                </FadeUp>
              )
            })}
          </div>
        </div>
      </section>

      {/* General Application CTA */}
      <section className="py-32 relative text-center">
        <div className="mx-auto max-w-3xl px-6 lg:px-8">
          <FadeUp>
            <h2 className="font-display text-4xl lg:text-5xl text-white mb-6">
              {t('cta_headline')}
            </h2>
            <p className="text-lg text-white-60 mb-10">
              {t('cta_body')}
            </p>
            <Button href="/contact" variant="secondary" size="lg">
              {t('cta_button')}
            </Button>
          </FadeUp>
        </div>
      </section>
    </main>
  )
}
