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
  
    return (
      <section className={cn('py-16 overflow-hidden border-t border-white/[0.04]', className)}>
        <div className="mx-auto max-w-7xl px-6 lg:px-8 mb-12 text-center">
          <FadeUp>
            <p className="text-[15px] font-medium tracking-[0.2em] uppercase text-white-40">
              Trusted by organizations nationwide
            </p>
            <p className="mt-4 text-base md:text-lg text-white-60 max-w-2xl mx-auto font-light leading-relaxed">
              {t('subtitle')}
            </p>
          </FadeUp>
        </div>

        <div className="mx-auto max-w-5xl px-6 lg:px-8">
          <FadeUp delay={0.2}>
            <div className="flex flex-wrap justify-center items-center gap-x-12 gap-y-10 md:gap-x-16">
              {clients.map((client, i) => (
                <div
                  key={`client-${client.name}-${i}`}
                  className="w-[100px] sm:w-[120px] h-[60px] flex items-center justify-center transition-all duration-300 hover:scale-110 cursor-pointer"
                >
                  <img
                    src={client.logo}
                    alt={client.name}
                    className={cn("max-w-full max-h-full object-contain opacity-70 hover:opacity-100 transition-opacity duration-300", client.className)}
                  />
                </div>
              ))}
            </div>
          </FadeUp>
        </div>
      </section>
  )
}
