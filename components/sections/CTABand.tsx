import { useTranslations } from 'next-intl'
import { Button } from '../ui/Button'
import { FadeUp } from '../animations/FadeUp'

export function CTABand() {
  const t = useTranslations('cta')

  return (
    <section className="relative py-32 md:py-48 bg-[#141413] text-[#F9F6F0] overflow-hidden flex flex-col items-center justify-center text-center">
      {/* Subtle top separator */}
      <div className="absolute top-0 inset-x-0 h-px bg-gradient-to-r from-transparent via-white/10 to-transparent" />
      
      <div className="mx-auto max-w-5xl px-6 lg:px-8 relative z-10">
        <FadeUp>
          <h2 className="font-display text-[clamp(48px,8vw,100px)] leading-[0.95] tracking-[-0.03em] mb-8 font-medium">
            {t('headline')}
          </h2>
          <p className="text-xl md:text-2xl text-[#F9F6F0]/60 max-w-2xl mx-auto mb-12 font-light tracking-wide">
            {t('sub')}
          </p>
          <Button 
            href="/contact" 
            className="bg-[#F9F6F0] text-[#141413] hover:bg-white rounded-full px-12 py-6 text-sm uppercase tracking-[0.15em] transition-all duration-500 transform hover:-translate-y-1 hover:shadow-2xl hover:shadow-white/20 font-semibold"
          >
            {t('button')}
          </Button>
        </FadeUp>
      </div>
    </section>
  )
}
