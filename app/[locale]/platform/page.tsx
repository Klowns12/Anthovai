'use client'

import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'
import { StaggerChildren, staggerItem } from '@/components/animations/StaggerChildren'
import { motion } from 'framer-motion'
import { Upload, SlidersHorizontal, Plug, ShieldCheck, ScanSearch, Languages } from 'lucide-react'

/**
 * The public page for the platform — the one product on this site a visitor can
 * sign up for and use the same afternoon.
 *
 * It is also, until now, the only way to find the way in: nothing on the
 * marketing site linked to `/signin` at all, so a returning customer had to
 * know the URL. Both doors are in the hero.
 *
 * Every claim here is one the platform actually keeps. "It says when it does
 * not know" is `strict_knowledge`, on by default; the four layers of isolation
 * are real and the innermost is row-level security; the Thai tokenisation is
 * why `crates/ingestion/src/tokens.rs` exists. Marketing copy that runs ahead
 * of the product is a promise the support inbox has to keep.
 */
export default function PlatformPage() {
  const t = useTranslations('platform_page')

  const steps = [
    { key: 'upload', icon: Upload },
    { key: 'build', icon: SlidersHorizontal },
    { key: 'connect', icon: Plug },
  ] as const

  const points = [
    { key: 'grounded', icon: ScanSearch },
    { key: 'isolation', icon: ShieldCheck },
    { key: 'thai', icon: Languages },
  ] as const

  return (
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

            <p className="font-display text-[clamp(34px,5.5vw,64px)] leading-[1.08] text-white max-w-4xl mx-auto">
              {t('hero_headline')}
            </p>

            <p className="text-white-60 text-lg leading-relaxed max-w-2xl mx-auto mt-8">
              {t('hero_body')}
            </p>

            <div className="flex flex-wrap items-center justify-center gap-4 mt-12">
              <Link
                href="/signup"
                className="bg-gold text-bg font-medium text-sm tracking-wide px-8 py-4 rounded-md hover:bg-gold-light transition-colors"
              >
                {t('cta_primary')}
              </Link>
              <Link
                href="/signin"
                className="text-sm tracking-wide px-8 py-4 rounded-md border border-white/[0.12] text-white-60 hover:text-white hover:border-white/[0.24] transition-colors"
              >
                {t('cta_secondary')}
              </Link>
            </div>
          </FadeUp>
        </div>
      </section>

      {/* Three steps */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4">
              {t('steps_label')}
            </p>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-16">
              {t('steps_title')}
            </h2>
          </FadeUp>

          <StaggerChildren className="grid gap-8 md:grid-cols-3">
            {steps.map((step, index) => {
              const Icon = step.icon
              return (
                <motion.div
                  key={step.key}
                  variants={staggerItem}
                  className="bg-bg-2 border border-white/[0.06] rounded-lg p-8"
                >
                  <div className="flex items-center gap-4 mb-6">
                    <Icon className="w-5 h-5 text-gold" strokeWidth={1.5} />
                    <span className="text-[11px] font-mono tracking-[0.2em] text-white-30">
                      {String(index + 1).padStart(2, '0')}
                    </span>
                  </div>
                  <h3 className="text-white text-lg mb-3 leading-snug">
                    {t(`steps.${step.key}.title`)}
                  </h3>
                  <p className="text-white-60 leading-relaxed">
                    {t(`steps.${step.key}.body`)}
                  </p>
                </motion.div>
              )
            })}
          </StaggerChildren>
        </div>
      </section>

      {/* What it is careful about */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-16 max-w-2xl">
              {t('points_title')}
            </h2>
          </FadeUp>

          <StaggerChildren className="grid gap-12 md:grid-cols-3">
            {points.map((point) => {
              const Icon = point.icon
              return (
                <motion.div key={point.key} variants={staggerItem}>
                  <Icon className="w-5 h-5 text-gold mb-5" strokeWidth={1.5} />
                  <h3 className="text-white text-lg mb-3 leading-snug">
                    {t(`points.${point.key}.title`)}
                  </h3>
                  <p className="text-white-60 leading-relaxed">
                    {t(`points.${point.key}.body`)}
                  </p>
                </motion.div>
              )
            })}
          </StaggerChildren>
        </div>
      </section>

      {/* Closing */}
      <section className="py-24">
        <div className="mx-auto max-w-7xl px-6 lg:px-8 text-center">
          <FadeUp>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white">
              {t('closing_title')}
            </h2>
            <p className="text-white-60 text-lg leading-relaxed max-w-xl mx-auto mt-6">
              {t('closing_body')}
            </p>
            <Link
              href="/signup"
              className="inline-block bg-gold text-bg font-medium text-sm tracking-wide px-8 py-4 rounded-md hover:bg-gold-light transition-colors mt-10"
            >
              {t('cta_primary')}
            </Link>
          </FadeUp>
        </div>
      </section>
    </main>
  )
}
