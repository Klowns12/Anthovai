'use client'

import { useTranslations } from 'next-intl'
import { FadeUp } from '../animations/FadeUp'
import { useState } from 'react'

export function SolutionsFAQ() {
  const t = useTranslations('solutions_page.faq')
  const [openIndex, setOpenIndex] = useState<number | null>(null)

  const items = ['q1', 'q2']

  const faqSchema = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    "mainEntity": items.map((q) => ({
      "@type": "Question",
      "name": t(`${q}.q`),
      "acceptedAnswer": {
        "@type": "Answer",
        "text": t(`${q}.a`)
      }
    }))
  }

  return (
    <div className="space-y-2">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(faqSchema) }}
      />
      {items.map((q, i) => (
        <FadeUp key={q} delay={i * 0.1}>
          <div className="border-b border-[#1A1A1A]/10 last:border-0 pb-2">
            <button
              className="w-full text-left flex justify-between items-center py-6 focus:outline-none"
              onClick={() => setOpenIndex(openIndex === i ? null : i)}
              aria-expanded={openIndex === i}
            >
              <span className="font-medium text-lg md:text-xl text-[#1A1A1A] pr-8">{t(`${q}.q`)}</span>
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
              <p className="text-[#1A1A1A]/70 text-base md:text-lg leading-relaxed max-w-3xl whitespace-pre-line">
                {t(`${q}.a`)}
              </p>
            </div>
          </div>
        </FadeUp>
      ))}
    </div>
  )
}
