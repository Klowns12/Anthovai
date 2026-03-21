'use client'

import { usePathname, useRouter } from '@/i18n/navigation'
import { useLocale } from 'next-intl'
import { useState, useRef, useEffect, useTransition } from 'react'
import { motion, AnimatePresence } from 'framer-motion'
import { ChevronDown } from 'lucide-react'
import { cn } from '@/lib/utils'

const languages = [
  { code: 'en', label: 'ENGLISH' },
  { code: 'th', label: 'ไทย' },
  { code: 'zh', label: '中文' },
  { code: 'ja', label: '日本語' },
  { code: 'fr', label: 'FRANÇAIS' },
  { code: 'de', label: 'DEUTSCH' },
  { code: 'ko', label: '한국어' },
  { code: 'es', label: 'ESPAÑOL' },
] as const

export function LanguageSwitcher() {
  const [open, setOpen] = useState(false)
  const [isPending, startTransition] = useTransition()
  const locale = useLocale()
  const router = useRouter()
  const pathname = usePathname()
  const ref = useRef<HTMLDivElement>(null)

  const currentLang = languages.find((l) => l.code === locale) ?? languages[0]

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [])

  function switchLocale(code: string) {
    setOpen(false)
    startTransition(() => {
      // @ts-ignore - next-intl navigation types might complain but options accept scroll
      router.replace(pathname, { locale: code, scroll: false })
    })
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        disabled={isPending}
        className={cn(
          'flex items-center gap-1.5 text-[11px] tracking-[0.15em] uppercase',
          'text-[#1A1A1A]/60 hover:text-[#1A1A1A] transition-colors',
          isPending && 'opacity-50 cursor-wait'
        )}
        aria-label="Select language"
        aria-expanded={open}
        role="combobox"
      >
        {currentLang.label}
        <ChevronDown
          size={12}
          className={cn('transition-transform duration-200', open && 'rotate-180')}
        />
      </button>

      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.15 }}
            className="absolute right-0 top-full mt-2 min-w-[140px] bg-bg-2 border border-white/[0.08] rounded-md overflow-hidden z-50 shadow-lg"
            role="listbox"
          >
            {languages.map((lang) => (
              <button
                key={lang.code}
                onClick={() => switchLocale(lang.code)}
                disabled={isPending}
                className={cn(
                  'w-full text-left px-4 py-2.5 text-[11px] tracking-[0.15em] uppercase transition-colors',
                  lang.code === locale
                    ? 'text-[#1A1A1A] bg-[#1A1A1A]/5 font-bold'
                    : 'text-[#1A1A1A]/60 hover:text-[#1A1A1A] hover:bg-[#1A1A1A]/[0.02]',
                )}
                role="option"
                aria-selected={lang.code === locale}
              >
                {lang.label}
              </button>
            ))}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}
