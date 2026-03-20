import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { Button } from '../ui/Button'
import { StaggerChildren, staggerItem } from '../animations/StaggerChildren'
import { FadeUp } from '../animations/FadeUp'
import { GlobeCanvas } from '../globe/GlobeCanvas'
import { Counter } from '../ui/Counter'

export function Hero() {
  const t = useTranslations('hero')

  return (
    <section className="relative min-h-screen pt-24 pb-16 flex items-center overflow-hidden">
      {/* Background elements */}
      <div className="absolute inset-0 dot-grid -z-10" />
      <div className="absolute inset-0 bg-radial-gradient from-bg/20 via-bg to-bg -z-10" />

      <div className="mx-auto max-w-7xl w-full px-6 lg:px-8">
        <div className="grid grid-cols-1 lg:grid-cols-[55fr_45fr] gap-12 lg:gap-8 items-center">
          {/* Left Column (Text) */}
          <div className="relative z-10 pt-12 lg:pt-0">
            <StaggerChildren>
              {/* Eyebrow */}
              <div className="mb-6 flex items-center gap-3">
                <span className="w-8 h-[1px] bg-gold" />
                <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('eyebrow')}
                </h2>
              </div>

              {/* Headline */}
              <h1 className="font-display text-[clamp(56px,7vw,104px)] leading-[0.9] tracking-[-0.02em] text-white my-6">
                <span className="block">{t('title_1')}</span>
                <span className="block italic text-gold">{t('title_2')}</span>
              </h1>

              {/* Body */}
              <p className="mt-8 max-w-xl text-lg leading-relaxed text-white-60 font-sans">
                {t('body')}
              </p>

              {/* CTAs */}
              <div className="mt-10 flex flex-wrap items-center gap-4">
                <Button href="/products" variant="primary" size="lg">
                  {t('cta_primary')}
                </Button>
                <Button href="/about" variant="ghost" size="lg">
                  {t('cta_secondary')}
                </Button>
              </div>

              {/* Stats Row */}
              <div className="mt-16 pt-8 border-t border-white/[0.08] flex gap-8 whitespace-nowrap overflow-x-auto no-scrollbar">
                <div className="flex flex-col">
                  <span className="font-display text-3xl text-white">
                    <Counter to={500} suffix="+" duration={2.5} />
                  </span>
                  <span className="text-[10px] uppercase tracking-widest text-white-30 mt-1">
                    {t('stat_projects')}
                  </span>
                </div>
                <div className="w-[1px] h-10 bg-white/[0.08]" />
                <div className="flex flex-col">
                  <span className="font-display text-3xl text-white">
                    <Counter to={99.99} suffix="%" duration={2.5} />
                  </span>
                  <span className="text-[10px] uppercase tracking-widest text-white-30 mt-1">
                    {t('stat_uptime')}
                  </span>
                </div>
                <div className="w-[1px] h-10 bg-white/[0.08]" />
                <div className="flex flex-col">
                  <span className="font-display text-3xl text-white">
                    <Counter to={28} duration={2.5} />
                  </span>
                  <span className="text-[10px] uppercase tracking-widest text-white-30 mt-1">
                    {t('stat_countries')}
                  </span>
                </div>
              </div>
            </StaggerChildren>
          </div>

          {/* Right Column (Globe) */}
          <div className="relative aspect-square w-full max-w-[600px] mx-auto lg:ml-auto">
            <div className="absolute inset-4 rounded-full bg-gold/[0.02] blur-3xl" />
            <GlobeCanvas className="absolute inset-0" />
            
            {/* Visual anchor dots */}
            <div className="absolute top-0 right-[20%] w-1.5 h-1.5 bg-gold rounded-full opacity-50 shadow-[0_0_10px_rgba(201,168,76,0.8)]" />
            <div className="absolute bottom-[10%] left-[10%] w-2 h-2 bg-gold rounded-full opacity-30 blur-[1px]" />
          </div>
        </div>
      </div>
    </section>
  )
}
