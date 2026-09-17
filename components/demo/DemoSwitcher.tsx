'use client'

import { useEffect, useState } from 'react'
import { useTranslations } from 'next-intl'
import { DeliveryNoteDemo } from '@/components/demo/DeliveryNoteDemo'
import { RagDemo } from '@/components/demo/RagDemo'

type Tab = 'ocr' | 'rag'

function tabFromHash() {
  if (typeof window === 'undefined') return 'ocr' as Tab
  return window.location.hash === '#rag' ? 'rag' : 'ocr'
}

export function DemoSwitcher() {
  const t = useTranslations('demo')
  const [tab, setTab] = useState<Tab>('ocr')

  useEffect(() => {
    const apply = () => setTab(tabFromHash())
    apply()
    window.addEventListener('hashchange', apply)
    return () => window.removeEventListener('hashchange', apply)
  }, [])

  function select(next: Tab) {
    setTab(next)
    const url = `${window.location.pathname}${window.location.search}${next === 'rag' ? '#rag' : '#ocr'}`
    window.history.replaceState(null, '', url)
  }

  return (
    <div className="pt-32 pb-24">
      <div className="mx-auto max-w-5xl px-6 lg:px-8">
        <div role="tablist" aria-label={t('label')} className="flex gap-2 border-b border-white/[0.08]">
          {(['ocr', 'rag'] as const).map((id) => {
            const selected = tab === id
            return (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={selected}
                onClick={() => select(id)}
                className={`px-4 py-3 text-sm tracking-wide transition-colors ${
                  selected
                    ? 'text-gold border-b-2 border-gold -mb-px'
                    : 'text-white-60 hover:text-white'
                }`}
              >
                {t(id === 'ocr' ? 'tab_ocr' : 'tab_rag')}
              </button>
            )
          })}
        </div>
      </div>
      {tab === 'ocr' ? <DeliveryNoteDemo /> : <RagDemo />}
    </div>
  )
}
