import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'

export function Manifesto() {
  const t = useTranslations('manifesto')

  // The specific text parsing for highlighting A and I
  const highlightParts = (text: string) => {
    return text.split(/(A|I)/g).map((part, i) => {
      if (part === 'A' || part === 'I') {
        return (
          <span key={i} className="text-gold">
            {part}
          </span>
        )
      }
      return <span key={i}>{part}</span>
    })
  }

  return (
    <section className="relative w-full py-40 lg:py-48 bg-bg overflow-hidden flex items-center justify-center">
      {/* Background glow & noise */}
      <div className="absolute inset-0 bg-radial-gradient from-gold/[0.05] via-transparent to-transparent opacity-80" />
      
      <div className="relative z-10 mx-auto max-w-4xl px-6 lg:px-8 text-center">
        <FadeUp>
          {/* Decorative quote mark */}
          <div className="absolute -top-16 lg:-top-24 left-1/2 -translate-x-1/2 font-display text-[140px] leading-none text-gold opacity-[0.06] select-none pointer-events-none">
            &ldquo;
          </div>
          
          <blockquote className="font-display italic text-[clamp(28px,4vw,52px)] leading-[1.1] tracking-[-0.01em] text-white">
            {highlightParts(t('quote'))}
          </blockquote>
          
          <div className="mt-12 lg:mt-16">
            <p className="font-sans text-[11px] font-medium tracking-[0.22em] uppercase text-white-30">
              {t('attribution')}
            </p>
          </div>
        </FadeUp>
      </div>
    </section>
  )
}
