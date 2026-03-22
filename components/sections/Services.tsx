import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'
import { Cpu, Terminal, Cloud, ShieldCheck, Database, Lightbulb, Smartphone, Settings, Palette } from 'lucide-react'

export function Services() {
  const t = useTranslations('services')

  const items = [
    { key: 'ai', icon: Cpu },
    { key: 'enterprise', icon: Terminal },
    { key: 'cloud', icon: Cloud },
    { key: 'security', icon: ShieldCheck },
    { key: 'data', icon: Database },
    { key: 'mobile', icon: Smartphone },
    { key: 'devops', icon: Settings },
    { key: 'ux', icon: Palette },
    { key: 'consulting', icon: Lightbulb },
  ] as const

  return (
    <section id="services" className="py-24 lg:py-32 bg-bg-2 border-y border-white/[0.04]">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <FadeUp>
          <div className="flex flex-col md:flex-row md:items-end justify-between gap-8 mb-16">
            <div>
              <div className="flex items-center gap-3 mb-6">
                <span className="w-8 h-[1px] bg-gold" />
                <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('label')}
                </h2>
              </div>
              <h3 className="font-display text-[clamp(40px,5vw,64px)] leading-[1.05] tracking-[-0.01em] text-white">
                {t('headline')}
              </h3>
            </div>
          </div>
        </FadeUp>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-y-12 gap-x-8">
          {items.map((item, i) => {
            const Icon = item.icon
            return (
              <FadeUp key={item.key} delay={i * 0.1}>
                <div className="p-8 border border-white/[0.06] bg-bg rounded-lg hover:border-gold-border hover:bg-gold/[0.02] transition-colors duration-300 h-full">
                  <Icon size={24} className="text-gold mb-6" />
                  <h4 className="text-lg font-medium text-white mb-3 tracking-tight">
                    {t(`items.${item.key}.title`)}
                  </h4>
                  <p className="text-white-60 leading-relaxed text-sm">
                    {t(`items.${item.key}.desc`)}
                  </p>
                </div>
              </FadeUp>
            )
          })}
        </div>
      </div>
    </section>
  )
}
