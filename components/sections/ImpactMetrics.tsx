'use client'

import { useTranslations } from 'next-intl'
import { motion } from 'framer-motion'
import { FadeUp } from '../animations/FadeUp'
import { StaggerChildren, staggerItem } from '../animations/StaggerChildren'
import { Activity, Users, Cpu, ShieldCheck } from 'lucide-react'

export function ImpactMetrics() {
  const t = useTranslations('impact')

  const metrics = [
    { key: 'm1', icon: ShieldCheck },
    { key: 'm2', icon: Users },
    { key: 'm3', icon: Activity },
    { key: 'm4', icon: Cpu },
  ] as const

  return (
    <section className="py-24 bg-[#F9F6F0] border-y border-white/[0.04]">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <FadeUp className="mb-16">
          <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4 block">
            {t('label')}
          </h2>
          <h3 className="font-display text-4xl md:text-5xl tracking-tight text-white max-w-2xl">
            {t('headline')}
          </h3>
        </FadeUp>

        <StaggerChildren className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-8 lg:gap-12 border-t border-white/[0.06] pt-12">
          {metrics.map((metric) => {
            const Icon = metric.icon
            return (
              <motion.div key={metric.key} variants={staggerItem} className="flex flex-col relative group">
                <div className="w-10 h-10 rounded-lg bg-bg border border-white/[0.08] flex items-center justify-center mb-6 group-hover:bg-gold/10 group-hover:border-gold/30 transition-colors duration-300">
                  <Icon size={20} className="text-gold" />
                </div>
                <div className="font-display text-5xl md:text-6xl tracking-tight text-white mb-4 group-hover:text-gold transition-colors duration-300">
                  {t(`metrics.${metric.key}.value`)}
                </div>
                <div className="text-white-60 text-sm leading-relaxed max-w-[200px]">
                  {t(`metrics.${metric.key}.label`)}
                </div>
              </motion.div>
            )
          })}
        </StaggerChildren>
      </div>
    </section>
  )
}
