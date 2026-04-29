import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'
import { Cpu, Cloud, Terminal } from 'lucide-react'

export function Services() {
  const t = useTranslations('services')

  const items = [
    { key: 'ai', icon: Cpu },
    { key: 'infrastructure', icon: Cloud },
    { key: 'platforms', icon: Terminal },
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

        <div className="grid grid-cols-1 md:grid-cols-3 gap-y-12 gap-x-8">
          {items.map((item, i) => {
            const Icon = item.icon
            const tags: string[] = t.raw(`clusters.${item.key}.tags`)

            return (
              <FadeUp key={item.key} delay={i * 0.1}>
                <div className="p-8 border border-white/[0.06] bg-bg rounded-lg hover:border-gold-border hover:bg-gold/[0.02] transition-colors duration-300 h-full flex flex-col">
                  <Icon size={24} className="text-gold mb-6" />
                  <h4 className="text-xl font-medium text-white mb-4 tracking-tight">
                    {t(`clusters.${item.key}.title`)}
                  </h4>
                  <p className="text-white-60 leading-relaxed text-sm mb-8 flex-grow">
                    {t(`clusters.${item.key}.desc`)}
                  </p>
                  <div className="flex flex-wrap gap-2 mt-auto">
                    {tags.map((tag, tagIndex) => (
                      <span key={tagIndex} className="px-3 py-1 bg-white/[0.03] border border-white/[0.06] text-white/70 text-xs rounded-full">
                        {tag}
                      </span>
                    ))}
                  </div>
                </div>
              </FadeUp>
            )
          })}
        </div>
      </div>
    </section>
  )
}
