'use client'

import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { StaggerChildren, staggerItem } from '@/components/animations/StaggerChildren'
import { Process } from '@/components/sections/Process'
import { Cpu, Server, Shield, Database, ArrowRight, Library, Factory, Building2, Activity, Plus } from 'lucide-react'
import Link from 'next/link'
import { motion } from 'framer-motion'

// Quick specific FAQ component for the solutions page
import { SolutionsFAQ } from '@/components/sections/SolutionsFAQ'

export default function SolutionsPage() {
  const t = useTranslations('solutions_page')
  const tServices = useTranslations('services')

  const services = [
    { key: 'ai', icon: Cpu },
    { key: 'infrastructure', icon: Server },
    { key: 'platforms', icon: Database },
  ] as const

  const industries = [
    { key: 'healthcare', icon: Activity },
    { key: 'education', icon: Library },
    { key: 'government', icon: Building2 },
    { key: 'manufacturing', icon: Factory },
  ] as const

  return (
    <>
      <main className="pt-32 pb-0">
        {/* Hero */}
        <section className="py-24 relative overflow-hidden border-b border-white/[0.04]">
          <div className="absolute inset-0 dot-grid opacity-20 pointer-events-none" />
          <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10 text-center">
            <FadeUp>
              <div className="flex items-center justify-center gap-3 mb-8">
                <span className="w-8 h-[1px] bg-gold" />
                <h1 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('hero_label')}
                </h1>
                <span className="w-8 h-[1px] bg-gold" />
              </div>
              <h2 className="font-display text-[clamp(48px,6vw,80px)] leading-[1.05] tracking-[-0.01em] text-white max-w-4xl mx-auto mb-8">
                {t('hero_headline')}
              </h2>
              <p className="text-xl text-white-60 leading-relaxed max-w-3xl mx-auto">
                {t('hero_body')}
              </p>
            </FadeUp>
          </div>
        </section>

        {/* LAYER 1: Services */}
        <section className="py-24 bg-bg border-b border-white/[0.04]">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp className="mb-16">
              <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-4 block">
                {t('services_label')}
              </h2>
              <h3 className="font-display text-4xl md:text-5xl tracking-tight text-white">
                {t('services_title')}
              </h3>
            </FadeUp>

            <StaggerChildren className="grid grid-cols-1 md:grid-cols-3 gap-6">
              {services.map((service) => {
                const Icon = service.icon
                const tags: string[] = tServices.raw(`clusters.${service.key}.tags`)
                
                return (
                  <motion.div key={service.key} variants={staggerItem} className="group p-8 border border-white/[0.06] bg-bg-2 rounded-xl hover:border-gold-border hover:bg-[#F0EFE9] transition-colors duration-300 flex flex-col h-full">
                    <div className="w-12 h-12 rounded-full border border-white/[0.08] bg-bg flex items-center justify-center mb-6 group-hover:border-gold group-hover:bg-[#E8E6E0] transition-colors duration-500">
                      <Icon size={20} className="text-white group-hover:text-gold transition-colors" />
                    </div>
                    <h4 className="text-xl font-medium text-white mb-3">
                      {tServices(`clusters.${service.key}.title`)}
                    </h4>
                    <p className="text-white-60 leading-relaxed text-sm flex-grow mb-6">
                      {tServices(`clusters.${service.key}.desc`)}
                    </p>
                    <div className="flex flex-wrap gap-2 mt-auto">
                      {tags.map((tag, tagIndex) => (
                        <span key={tagIndex} className="px-3 py-1 bg-white/[0.03] border border-white/[0.06] text-white/70 text-xs rounded-full group-hover:border-black/10 group-hover:bg-black/5 group-hover:text-black/70 transition-colors">
                          {tag}
                        </span>
                      ))}
                    </div>
                  </motion.div>
                )
              })}
            </StaggerChildren>
          </div>
        </section>

        {/* LAYER 2: Industries */}
        <section className="py-24 bg-bg border-b border-white/[0.04]">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp className="mb-16">
              <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-4 block">
                {t('industries_label')}
              </h2>
              <h3 className="font-display text-4xl md:text-5xl tracking-tight text-white">
                {t('industries_title')}
              </h3>
            </FadeUp>

            <StaggerChildren className="grid grid-cols-1 md:grid-cols-2 gap-8">
              {industries.map((industry) => {
                const Icon = industry.icon
                const tags: string[] = t.raw(`industries.${industry.key}.tags`)
                return (
                  <motion.div key={industry.key} variants={staggerItem} className="group p-10 border border-[#FFFFFF]/10 bg-[#111111] rounded-xl hover:border-gold-border hover:bg-[#1A1814] transition-colors duration-300">
                    <div className="flex items-start gap-6">
                      <div className="w-14 h-14 rounded-full border border-[#FFFFFF]/15 bg-bg flex items-center justify-center shrink-0 group-hover:border-gold group-hover:bg-[#F0EFE9] transition-colors duration-500">
                        <Icon size={24} className="text-[#06060A] group-hover:text-gold transition-colors" />
                      </div>
                      <div className="flex-1">
                        <h4 className="text-2xl font-medium text-[#FFFFFF] mb-4">
                          {t(`industries.${industry.key}.title`)}
                        </h4>
                        <div className="text-[#FFFFFF]/60 leading-relaxed space-y-2 whitespace-pre-line">
                          {t(`industries.${industry.key}.desc`)}
                        </div>
                        <div className="mt-6 pl-4 border-l-2 border-gold/30">
                          <p className="text-[#FFFFFF]/90 text-sm leading-relaxed italic">
                            "{t(`industries.${industry.key}.did`)}"
                          </p>
                        </div>
                        {tags && tags.length > 0 && (
                          <div className="flex flex-wrap gap-2 mt-6 pt-6 border-t border-[#FFFFFF]/10">
                            {tags.map((tag, tagIndex) => (
                              <span key={tagIndex} className="px-3 py-1 bg-white/[0.03] border border-white/[0.06] text-white/70 text-xs rounded-full">
                                {tag}
                              </span>
                            ))}
                          </div>
                        )}
                      </div>
                    </div>
                  </motion.div>
                )
              })}
            </StaggerChildren>
          </div>
        </section>

        {/* LAYER 3: Process (How We Work) */}
        {/* The Process component has its own padding and border logic. */}
        <Process />

        {/* FAQ & CTA Section */}
        <section className="py-24 bg-[#F9F6F0] relative" id="faq-cta">
          <div className="mx-auto max-w-4xl px-6 lg:px-8">
            <FadeUp>
              <div className="text-center mb-16">
                <h2 className="font-display text-[clamp(32px,5vw,56px)] tracking-tight text-[#1A1A1A]">
                  {t('faq_title')}
                </h2>
              </div>
            </FadeUp>
            
            <SolutionsFAQ />

            <FadeUp className="mt-32">
              <div className="bg-[#1A1A1A] rounded-2xl p-12 md:p-16 text-center shadow-2xl">
                <h2 className="font-display text-3xl md:text-5xl text-[#FFFFFF] tracking-tight mb-6">
                  {t('cta.headline')}
                </h2>
                <p className="text-lg text-[#FFFFFF]/70 mb-10 max-w-2xl mx-auto">
                  {t('cta.sub')}
                </p>
                <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
                  <Link 
                    href="/contact"
                    className="group relative inline-flex items-center justify-center w-full sm:w-auto px-8 py-4 bg-[#FFFFFF] text-[#06060A] font-medium text-sm tracking-wide overflow-hidden min-w-[200px] hover:bg-[#F0EFE9] transition-colors"
                  >
                    <span className="relative z-10 flex items-center gap-2">
                      {t('cta.button_consultation')}
                      <ArrowRight size={16} className="group-hover:translate-x-1 transition-transform" />
                    </span>
                  </Link>
                  <Link 
                    href="/contact"
                    className="group relative inline-flex items-center justify-center w-full sm:w-auto px-8 py-4 bg-transparent border border-[#FFFFFF]/30 text-[#FFFFFF] font-medium text-sm tracking-wide overflow-hidden min-w-[200px] hover:border-[#FFFFFF]/60 transition-colors"
                  >
                    <span className="relative z-10 flex items-center gap-2">
                      {t('cta.button_estimate')}
                    </span>
                  </Link>
                </div>
              </div>
            </FadeUp>
          </div>
        </section>

      </main>
    </>
  )
}
