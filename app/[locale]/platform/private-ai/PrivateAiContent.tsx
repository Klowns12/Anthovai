'use client'

import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { motion } from 'framer-motion'
import {
  ArrowRight,
  FileStack,
  ScanText,
  Database,
  MessageSquareQuote,
} from 'lucide-react'
import { FadeUp } from '@/components/animations/FadeUp'
import { StaggerChildren, staggerItem } from '@/components/animations/StaggerChildren'

type Titled = { title: string; body: string }
type CompareRow = { label: string; public: string; private: string }
type Week = { when: string; title: string; body: string }

/**
 * Every claim on this page is one the platform keeps today.
 *
 * "Installed on a server you control" is `docker/docker-compose.stack.yml`;
 * "reads scanned Thai documents and photographs" is the OCR sidecar and the
 * image source type in `crates/ingestion`; "every answer cites its page" is
 * the `page` on every chunk's metadata; "access by workspace" is the
 * permission model; "conversations kept in your own database" is the
 * `conversation` crate writing to the customer's PostgreSQL. The one thing
 * deliberately *not* promised is perfect OCR — the benchmark in
 * `platform/ocr-sidecar/docs` says 97% on clean pages, and the page says
 * "reads", not "reads flawlessly".
 */
export function PrivateAiContent() {
  const t = useTranslations('private_ai_page')

  const boundary = [
    { key: 'docs', icon: FileStack },
    { key: 'read', icon: ScanText },
    { key: 'kb', icon: Database },
    { key: 'answer', icon: MessageSquareQuote },
  ] as const

  const why = ['damage', 'pdpa', 'shadow'] as const
  const uses = t.raw('uses') as Titled[]
  const compareRows = t.raw('compare_rows') as CompareRow[]
  const weeks = t.raw('weeks') as Week[]
  const tiers = ['pilot', 'business', 'enterprise'] as const

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

            <p className="font-display text-[clamp(34px,5.5vw,64px)] leading-[1.08] text-white max-w-4xl mx-auto [text-wrap:balance]">
              {t('hero_headline')}
            </p>

            <p className="text-white-60 text-lg leading-relaxed max-w-2xl mx-auto mt-8">
              {t('hero_body')}
            </p>

            <div className="flex flex-wrap items-center justify-center gap-4 mt-12">
              <Link
                href="/contact"
                className="bg-gold text-bg font-medium text-sm tracking-wide px-8 py-4 rounded-md hover:bg-gold-light transition-colors"
              >
                {t('cta_primary')}
              </Link>
              <Link
                href="/platform"
                className="text-sm tracking-wide px-8 py-4 rounded-md border border-white/[0.12] text-white-60 hover:text-white hover:border-white/[0.24] transition-colors"
              >
                {t('cta_secondary')}
              </Link>
            </div>
          </FadeUp>
        </div>
      </section>

      {/* The boundary — the one picture the whole offer rests on */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <div className="relative rounded-lg border-2 border-dashed border-gold/40 bg-gold-dim px-6 pt-10 pb-7 md:px-8">
              <span className="absolute -top-3 left-6 bg-bg px-3 text-[11px] font-mono tracking-[0.18em] uppercase text-gold">
                {t('boundary_label')}
              </span>

              <div className="grid gap-3 md:grid-cols-[1fr_auto_1fr_auto_1fr_auto_1fr] md:items-stretch">
                {boundary.map((node, index) => {
                  const Icon = node.icon
                  return (
                    <div key={node.key} className="contents">
                      <div className="bg-bg border border-white/[0.06] rounded-md p-5">
                        <Icon className="w-5 h-5 text-gold mb-4" strokeWidth={1.5} />
                        <h3 className="text-white font-medium leading-snug">
                          {t(`boundary.${node.key}.title`)}
                        </h3>
                        <p className="text-white-60 text-sm leading-relaxed mt-2">
                          {t(`boundary.${node.key}.body`)}
                        </p>
                      </div>
                      {index < boundary.length - 1 && (
                        <ArrowRight
                          className="w-4 h-4 text-gold self-center justify-self-center rotate-90 md:rotate-0"
                          strokeWidth={2}
                          aria-hidden
                        />
                      )}
                    </div>
                  )
                })}
              </div>

              <div className="flex flex-wrap items-center justify-between gap-3 mt-6 text-sm">
                <span className="text-white-60">{t('boundary_inside')}</span>
                <span className="font-mono text-[12px] tracking-wide text-gold">
                  {t('boundary_outside')}
                </span>
              </div>
            </div>
          </FadeUp>
        </div>
      </section>

      {/* Why now */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4">
              {t('why_label')}
            </p>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-16 max-w-3xl [text-wrap:balance]">
              {t('why_title')}
            </h2>
          </FadeUp>

          <StaggerChildren className="grid gap-8 md:grid-cols-3">
            {why.map((key) => (
              <motion.div
                key={key}
                variants={staggerItem}
                className="bg-bg-2 border border-white/[0.06] rounded-lg p-8"
              >
                <div className="font-mono text-2xl text-gold leading-none mb-5">
                  {t(`why.${key}.stat`)}
                </div>
                <h3 className="text-white text-lg mb-3 leading-snug">
                  {t(`why.${key}.title`)}
                </h3>
                <p className="text-white-60 leading-relaxed">{t(`why.${key}.body`)}</p>
              </motion.div>
            ))}
          </StaggerChildren>
        </div>
      </section>

      {/* What the team can do */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4">
              {t('uses_label')}
            </p>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-14 max-w-2xl">
              {t('uses_title')}
            </h2>
          </FadeUp>

          <StaggerChildren className="grid md:grid-cols-2 border-t border-white/[0.06]">
            {uses.map((use, index) => (
              <motion.div
                key={use.title}
                variants={staggerItem}
                className="flex gap-5 py-7 pr-8 border-b border-white/[0.06]"
              >
                <span className="text-[11px] font-mono tracking-[0.2em] text-white-30 pt-1.5 shrink-0">
                  {String(index + 1).padStart(2, '0')}
                </span>
                <div>
                  <h3 className="text-white font-medium leading-snug">{use.title}</h3>
                  <p className="text-white-60 leading-relaxed mt-2">{use.body}</p>
                </div>
              </motion.div>
            ))}
          </StaggerChildren>
        </div>
      </section>

      {/* Side by side */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4">
              {t('compare_label')}
            </p>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-14 max-w-2xl">
              {t('compare_title')}
            </h2>

            <div className="overflow-x-auto">
              <table className="w-full min-w-[640px] border-collapse text-[15px]">
                <thead>
                  <tr className="border-b-2 border-white">
                    <th className="text-left font-normal py-3 pr-4" />
                    <th className="text-left font-medium py-3 px-4 text-white">
                      {t('compare_public')}
                    </th>
                    <th className="text-left font-medium py-3 px-4 text-white bg-gold-dim">
                      {t('compare_private')}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {compareRows.map((row) => (
                    <tr key={row.label} className="border-b border-white/[0.06]">
                      <td className="py-4 pr-4 text-white-60 align-top">{row.label}</td>
                      <td className="py-4 px-4 text-white-60 align-top">{row.public}</td>
                      <td className="py-4 px-4 text-white font-medium align-top bg-gold-dim">
                        {row.private}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </FadeUp>
        </div>
      </section>

      {/* Packages */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4">
              {t('packages_label')}
            </p>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-14 max-w-2xl">
              {t('packages_title')}
            </h2>
          </FadeUp>

          <StaggerChildren className="grid gap-6 md:grid-cols-3 items-start">
            {tiers.map((tier) => {
              const points = t.raw(`tiers.${tier}.points`) as string[]
              const monthly = t(`tiers.${tier}.monthly`)
              const lead = tier === 'business'
              return (
                <motion.div
                  key={tier}
                  variants={staggerItem}
                  className={
                    lead
                      ? 'bg-bg-2 border border-gold-border shadow-gold rounded-lg p-8'
                      : 'bg-bg-2 border border-white/[0.06] rounded-lg p-8'
                  }
                >
                  <h3 className="text-white text-lg font-medium">{t(`tiers.${tier}.name`)}</h3>
                  <p className="text-white-30 text-sm mt-1">{t(`tiers.${tier}.for`)}</p>

                  <div className="mt-6 font-display text-3xl text-white tabular-nums">
                    {t(`tiers.${tier}.price`)}
                  </div>
                  <p className="text-white-30 text-xs tracking-wide uppercase mt-1">
                    {t(`tiers.${tier}.price_note`)}
                  </p>
                  {monthly && <p className="text-white-60 text-sm mt-3">{monthly}</p>}

                  <ul className="mt-6 pt-6 border-t border-white/[0.06] space-y-2.5">
                    {points.map((point) => (
                      <li key={point} className="flex gap-3 text-sm text-white-60 leading-relaxed">
                        <span className="text-gold shrink-0">·</span>
                        {point}
                      </li>
                    ))}
                  </ul>
                </motion.div>
              )
            })}
          </StaggerChildren>

          <FadeUp>
            <p className="text-white-30 text-sm mt-8 max-w-3xl">{t('packages_note')}</p>
          </FadeUp>
        </div>
      </section>

      {/* The first 30 days */}
      <section className="py-24 border-b border-white/[0.04]">
        <div className="mx-auto max-w-7xl px-6 lg:px-8">
          <FadeUp>
            <p className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold mb-4">
              {t('timeline_label')}
            </p>
            <h2 className="font-display text-[clamp(26px,3.5vw,42px)] leading-tight text-white mb-14 max-w-2xl">
              {t('timeline_title')}
            </h2>
          </FadeUp>

          <StaggerChildren className="grid md:grid-cols-4">
            {weeks.map((week) => (
              <motion.div
                key={week.when}
                variants={staggerItem}
                className="border-l-2 border-gold/40 pl-6 pb-8 md:pb-0 md:pr-8"
              >
                <div className="text-[11px] font-mono tracking-[0.2em] uppercase text-gold">
                  {week.when}
                </div>
                <h3 className="text-white font-medium mt-2 leading-snug">{week.title}</h3>
                <p className="text-white-60 text-sm leading-relaxed mt-2">{week.body}</p>
              </motion.div>
            ))}
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
              href="/contact"
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
