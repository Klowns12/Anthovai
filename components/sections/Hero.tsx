import { useTranslations } from 'next-intl'
import { Button } from '../ui/Button'
import { StaggerChildren } from '../animations/StaggerChildren'
import { GlobeCanvas } from '../globe/GlobeCanvas'

export function Hero() {
  const t = useTranslations('hero')

  return (
    <section className="relative h-screen flex items-center overflow-hidden">
      {/* Background elements */}
      <div className="absolute inset-0 bg-[#F2F0E9] -z-20" />

      {/* Background Globe — fills entire viewport */}
      <div className="absolute inset-0 flex items-center justify-center z-0 pointer-events-none">
        <div className="relative w-full h-screen max-w-[1400px] flex items-center justify-center pointer-events-auto">
          <GlobeCanvas className="absolute inset-0 w-full h-full" />
        </div>
      </div>

      <div className="mx-auto max-w-7xl w-full px-6 lg:px-8 relative z-10 pt-16">
        <div className="max-w-3xl mx-auto flex flex-col items-center justify-center text-center relative py-12">
          {/* Soft background halo for text readability */}
          <div className="absolute inset-0 bg-[radial-gradient(circle_at_center,rgba(242,240,233,0.85)_0%,rgba(242,240,233,0.6)_35%,rgba(242,240,233,0)_65%)] -z-10 pointer-events-none rounded-full blur-xl" />
          <StaggerChildren>
            {/* Eyebrow */}
            <div className="mb-6 flex items-center justify-center gap-3">
              <h2 className="text-[11px] font-semibold tracking-[0.2em] uppercase text-[#4A4A4A]">
                {t('eyebrow')}
              </h2>
            </div>

            {/* Headline */}
            <h1 className="font-display text-[clamp(44px,6vw,90px)] leading-[1.0] tracking-[-0.03em] text-[#1A1A1A] my-6">
              <span className="block">{t('title_1')}</span>
              <span className="block text-[#1A1A1A]">{t('title_2')}</span>
            </h1>

            {/* Body */}
            <p className="mt-8 max-w-xl mx-auto text-[17px] leading-relaxed text-[#4A4A4A] font-sans">
              {t('body')}
            </p>

            {/* CTAs */}
            <div className="mt-10 flex flex-wrap justify-center items-center gap-4">
              <Button href="#services" variant="ghost" className="rounded-full border border-[#1A1A1A]/20 text-[#1A1A1A] hover:bg-[#1A1A1A]/5 px-8 pt-[3px] pb-1 transition-colors">
                {t('cta_primary')}
              </Button>
            </div>
          </StaggerChildren>
        </div>
      </div>
    </section>
  )
}
