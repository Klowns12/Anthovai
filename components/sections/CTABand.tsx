import { useTranslations } from 'next-intl'
import { Button } from '../ui/Button'
import { FadeUp } from '../animations/FadeUp'

export function CTABand() {
  const t = useTranslations('cta')

  return (
    <section className="py-24 bg-gold relative overflow-hidden">
      {/* Texture overlay on the gold */}
      <div className="absolute inset-0 opacity-10 mix-blend-multiply" style={{ backgroundImage: 'url("data:image/svg+xml,%3Csvg viewBox=\'0 0 256 256\' xmlns=\'http://www.w3.org/2000/svg\'%3E%3Cfilter id=\'noise\'%3E%3CfeTurbulence type=\'fractalNoise\' baseFrequency=\'0.9\' numOctaves=\'4\' stitchTiles=\'stitch\'/%3E%3C/filter%3E%3Crect width=\'100%25\' height=\'100%25\' filter=\'url(%23noise)\' opacity=\'0.5\'/%3E%3C/svg%3E")' }} />
      
      <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10">
        <FadeUp>
          <div className="flex flex-col md:flex-row md:items-center justify-between gap-8 py-8">
            <div className="max-w-2xl">
              <h2 className="font-display text-[clamp(40px,5vw,64px)] leading-[1.05] tracking-[-0.02em] text-bg mb-4">
                {t('headline')}
              </h2>
              <p className="text-xl text-bg/70 font-medium">
                {t('sub')}
              </p>
            </div>
            
            <div className="shrink-0">
              <Button 
                href="/contact" 
                className="bg-bg text-gold hover:bg-bg-2 border-none px-10 py-5 text-sm"
              >
                {t('button')}
              </Button>
            </div>
          </div>
        </FadeUp>
      </div>
    </section>
  )
}
