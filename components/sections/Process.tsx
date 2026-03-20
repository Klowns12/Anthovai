'use client'

import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'
import { StaggerChildren, staggerItem } from '../animations/StaggerChildren'
import { Search, PenTool, Braces, Rocket } from 'lucide-react'
import { motion } from 'framer-motion'

export function Process() {
  const t = useTranslations('process')

  const steps = [
    { key: 'discover', icon: Search, number: '01' },
    { key: 'architect', icon: PenTool, number: '02' },
    { key: 'build', icon: Braces, number: '03' },
    { key: 'evolve', icon: Rocket, number: '04' },
  ] as const

  return (
    <section className="py-24 lg:py-32 bg-bg relative">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <FadeUp>
          <div className="mb-20">
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
        </FadeUp>

        <StaggerChildren className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-x-8 gap-y-16">
          {steps.map((step) => {
            const Icon = step.icon
            return (
              <motion.div key={step.key} variants={staggerItem} className="relative group">
                {/* Connecting line for desktop */}
                {step.number !== '04' && (
                  <div className="hidden lg:block absolute top-6 flex-1 w-[calc(100%-48px)] left-[60px] h-[1px] bg-white/[0.06]" />
                )}
                
                <div className="flex flex-col gap-6 relative z-10">
                  <div className="w-12 h-12 rounded-full border border-white/[0.08] bg-bg-2 flex items-center justify-center shrink-0 group-hover:border-gold group-hover:bg-gold/[0.05] transition-colors duration-500">
                    <Icon size={20} className="text-gold" />
                  </div>
                  
                  <div>
                    <div className="font-mono text-sm text-gold mb-3">{step.number} //</div>
                    <h4 className="text-xl font-medium text-white mb-3 tracking-tight">
                      {t(`steps.${step.key}.title`)}
                    </h4>
                    <p className="text-white-60 leading-relaxed text-sm">
                      {t(`steps.${step.key}.desc`)}
                    </p>
                  </div>
                </div>
              </motion.div>
            )
          })}
        </StaggerChildren>
      </div>
    </section>
  )
}
