'use client'

import { motion } from 'framer-motion'
import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'
import { cn } from '@/lib/utils'

const clients = [
  { name: 'CH', logo: '/client logo/ch-logo-Photoroom.png', className: 'scale-75' },
  { name: 'Digital Nova', logo: '/client logo/RGB-Digital Nova_final logo.png', className: 'scale-150' },
  { name: 'PMY', logo: '/client logo/pmy-logo-Photoroom.png', className: 'scale-120' },
  { name: 'Roi Et Bajabhat University', logo: '/client logo/rbu-logo-Photoroom.png', className: 'scale-90' },
  { name: 'Bellco', logo: '/client logo/bellco-logo.png', className: 'scale-100' },
  { name: 'Vista', logo: '/client logo/vista-logo-Photoroom.png', className: 'scale-120' },
  { name: 'Sang Thong', logo: '/client logo/44230-Photoroom.png', className: 'scale-160' },
  
]

  export function ClientLogos({ className }: { className?: string }) {
    const t = useTranslations('client_logos') 
  
    // Duplicate the array 4 times so we have enough width for a seamless infinite scroll
    const doubledClients = [...clients, ...clients, ...clients, ...clients]
  
    return (
      <section className={cn('py-16 overflow-hidden border-t border-white/[0.04]', className)}>
        <div className="mx-auto max-w-7xl px-6 lg:px-8 mb-10 text-center">
          <FadeUp>
            <p className="text-[15px] font-medium tracking-[0.2em] uppercase text-white-40">
              Trusted by organizations nationwide
            </p>
            <p className="mt-4 text-base md:text-lg text-white-60 max-w-2xl mx-auto font-light leading-relaxed">
              {t('subtitle')}
            </p>
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
            <div className="flex items-center gap-16 pr-16 w-max group-hover:[animation-play-state:paused]">
              {doubledClients.map((client, i) => (
                <div
                  key={`first-${client.name}-${i}`}
                  className="flex-shrink-0 w-[120px] h-[60px] flex items-center justify-center transition-all duration-300 hover:scale-110 cursor-pointer"
                >
                  <img
                    src={client.logo}
                    alt={client.name}
                    className={cn("max-w-full max-h-full object-contain opacity-80 hover:opacity-100 transition-opacity duration-300", client.className)}
                  />
                </div>
              ))}
            </div>
            {/* Second half */}
            <div className="flex items-center gap-16 pr-16 w-max aria-hidden group-hover:[animation-play-state:paused]">
              {doubledClients.map((client, i) => (
                <div
                  key={`second-${client.name}-${i}`}
                  className="flex-shrink-0 w-[120px] h-[60px] flex items-center justify-center transition-all duration-300 hover:scale-110 cursor-pointer"
                >
                  <img
                    src={client.logo}
                    alt={client.name}
                    className={cn("max-w-full max-h-full object-contain opacity-80 hover:opacity-100 transition-opacity duration-300", client.className)}
                  />
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
