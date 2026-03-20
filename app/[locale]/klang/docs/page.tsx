'use client'

import { klangDocs } from '@/content/klang-docs'
import { MarkdownContent } from '@/components/ui/MarkdownContent'
import { FadeUp } from '@/components/animations/FadeUp'
import { useEffect, useState } from 'react'

export default function KlangDocsPage() {
  const [activeId, setActiveId] = useState<string>('')

  // Intersection Observer to highlight active sidebar link
  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            setActiveId(entry.target.id)
          }
        })
      },
      { rootMargin: '0px 0px -80% 0px' }
    )

    klangDocs.forEach((doc) => {
      const element = document.getElementById(doc.id)
      if (element) observer.observe(element)
    })

    return () => observer.disconnect()
  }, [])

  return (
    <main className="pt-24 pb-24 min-h-screen bg-bg">
      {/* Header */}
      <div className="border-b border-white/[0.04] bg-bg-2">
        <div className="mx-auto max-w-[90rem] px-6 lg:px-8 py-12">
          <FadeUp>
            <h1 className="text-3xl font-display text-white mb-2">Klang Documentation</h1>
            <p className="text-white-60">Complete language reference, tools, and architecture.</p>
          </FadeUp>
        </div>
      </div>

      <div className="mx-auto max-w-[90rem] px-6 lg:px-8 flex flex-col lg:flex-row gap-12 mt-12 relative">
        
        {/* Sidebar TOC */}
        <aside className="lg:w-64 shrink-0 hidden lg:block">
          <div className="sticky top-32 max-h-[calc(100vh-8rem)] overflow-y-auto pr-6 custom-scrollbar flex flex-col gap-2">
            <h4 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-4 px-3">
              Contents
            </h4>
            <nav className="flex flex-col gap-1">
              {klangDocs.map((doc) => (
                <a
                  key={doc.id}
                  href={`#${doc.id}`}
                  className={`text-sm py-2 px-3 rounded-md transition-colors ${
                    activeId === doc.id
                      ? 'bg-gold/[0.08] text-gold font-medium'
                      : 'text-white-60 hover:text-white hover:bg-white/[0.04]'
                  }`}
                  onClick={(e) => {
                    e.preventDefault()
                    document.getElementById(doc.id)?.scrollIntoView({ behavior: 'smooth' })
                  }}
                >
                  {doc.title}
                </a>
              ))}
            </nav>
          </div>
        </aside>

        {/* Main Content Areas */}
        <div className="flex-1 max-w-4xl">
          {klangDocs.map((doc, index) => (
            <section 
              key={doc.id} 
              id={doc.id} 
              className="scroll-mt-32 mb-20 pb-12 border-b border-white/[0.04] last:border-b-0"
            >
              <FadeUp delay={index > 0 ? 0 : 0.2}>
                <h2 className="text-2xl font-display text-white mb-8 group flex items-center gap-3">
                  <span className="text-gold opacity-50 text-xl group-hover:opacity-100 transition-opacity">#</span>
                  {doc.title}
                </h2>
                <MarkdownContent content={doc.content} />
              </FadeUp>
            </section>
          ))}
        </div>

      </div>
    </main>
  )
}
