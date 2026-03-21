'use client'

import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'
import { useState } from 'react'

export function FAQ() {
  const t = useTranslations('faq')
  const [openIndex, setOpenIndex] = useState<number | null>(null)

  const items = ['q1', 'q2', 'q3', 'q4']

  const faqSchema = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    "mainEntity": items.map((q) => ({
      "@type": "Question",
      "name": t(`items.${q}.q`),
      "acceptedAnswer": {
        "@type": "Answer",
        "text": t(`items.${q}.a`)
      }
    }))
  }

  return (
    <section className="py-24 bg-[#F9F6F0] relative" id="faq">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(faqSchema) }}
      />
      <div className="mx-auto max-w-4xl px-6 lg:px-8">
        <FadeUp>
          <div className="text-center mb-16">
            <span className="text-[11px] font-medium tracking-[0.15em] uppercase text-[#1A1A1A]/60 mb-4 block">
              {t('label')}
            </span>
            <h2 className="font-display text-[clamp(32px,5vw,56px)] tracking-tight text-[#1A1A1A]">
              {t('headline')}
            </h2>
          </div>
          
          <div className="space-y-2">
            {items.map((q, i) => (
              <div key={q} className="border-b border-[#1A1A1A]/10 last:border-0 pb-2">
                <button
                  className="w-full text-left flex justify-between items-center py-6 focus:outline-none"
                  onClick={() => setOpenIndex(openIndex === i ? null : i)}
                  aria-expanded={openIndex === i}
                >
                  <span className="font-medium text-lg md:text-xl text-[#1A1A1A] pr-8">{t(`items.${q}.q`)}</span>
                  <span 
                    className="text-2xl font-light text-[#1A1A1A]/40 transition-transform duration-300 transform origin-center" 
                    style={{ transform: openIndex === i ? 'rotate(45deg)' : 'rotate(0deg)' }}
                  >
                    +
                  </span>
                </button>
                <div 
                  className={`overflow-hidden transition-all duration-300 ease-in-out ${openIndex === i ? 'max-h-96 opacity-100 pb-6' : 'max-h-0 opacity-0'}`}
                >
                  <p className="text-[#1A1A1A]/70 text-base md:text-lg leading-relaxed max-w-3xl">
                    {t(`items.${q}.a`)}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </FadeUp>
      </div>
    </section>
  )
}
