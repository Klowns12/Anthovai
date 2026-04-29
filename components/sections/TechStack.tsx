'use client'

import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'

const techStack = [
  'TypeScript', 'Next.js', 'React', 'Node.js', 'Python', 'Go', 
  'PostgreSQL', 'Redis', 'Docker', 'Kubernetes', 'AWS', 'TensorFlow', 'PyTorch', 'Klang'
]

export function TechStack() {
  const t = useTranslations('tech_stack')

  return (
    <section className="py-16 border-t border-white/[0.04] bg-bg overflow-hidden relative">
      <div className="mx-auto max-w-7xl px-6 lg:px-8 mb-10 text-center">
        <FadeUp>
          <div className="flex items-center justify-center gap-3">
            <span className="w-8 h-[1px] bg-gold" />
            <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
              {t('label')}
            </h2>
            <span className="w-8 h-[1px] bg-gold" />
          </div>
          <h3 className="mt-4 text-base md:text-lg text-white-60 font-light tracking-wide">
            {t('headline')}
          </h3>
        </FadeUp>
      </div>

      <div className="relative w-full flex items-center">
        {/* Left fade gradient */}
        <div className="absolute left-0 top-0 bottom-0 w-32 bg-gradient-to-r from-bg to-transparent z-10 pointer-events-none" />

        {/* Right fade gradient */}
        <div className="absolute right-0 top-0 bottom-0 w-32 bg-gradient-to-l from-bg to-transparent z-10 pointer-events-none" />

        <div className="flex overflow-hidden w-full group">
          <div
            className="flex w-max"
            style={{ animation: `marquee 40s linear infinite` }}
          >
            {/* First half */}
            <div className="flex items-center gap-6 pr-6 w-max group-hover:[animation-play-state:paused]">
              {[...techStack, ...techStack].map((tech, i) => (
                <div
                  key={`first-${tech}-${i}`}
                  className="flex-shrink-0 px-8 py-3 border border-white/[0.06] rounded-full bg-white/[0.02] text-white/80 text-sm font-medium tracking-wide hover:border-gold/30 hover:bg-gold/[0.02] hover:text-white transition-all cursor-default"
                >
                  {tech}
                </div>
              ))}
            </div>
            {/* Second half */}
            <div className="flex items-center gap-6 pr-6 w-max aria-hidden group-hover:[animation-play-state:paused]">
              {[...techStack, ...techStack].map((tech, i) => (
                <div
                  key={`second-${tech}-${i}`}
                  className="flex-shrink-0 px-8 py-3 border border-white/[0.06] rounded-full bg-white/[0.02] text-white/80 text-sm font-medium tracking-wide hover:border-gold/30 hover:bg-gold/[0.02] hover:text-white transition-all cursor-default"
                >
                  {tech}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
