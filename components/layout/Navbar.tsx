'use client'

import { useState, useEffect } from 'react'
import { useTranslations } from 'next-intl'
import { Link, usePathname } from '@/i18n/navigation'
import { motion, useScroll, useMotionValueEvent, AnimatePresence } from 'framer-motion'
import { Menu, X } from 'lucide-react'
import { LanguageSwitcher } from './LanguageSwitcher'
import { cn } from '@/lib/utils'

const navLinks = [
  // { href: '/products', key: 'products' },
  { href: '/solutions', key: 'solutions' },
  { href: '/klang', key: 'klang' },
  { href: '/about', key: 'about' },
  { href: '/careers', key: 'careers' },
] as const

export function Navbar() {
  const t = useTranslations('nav')
  const pathname = usePathname()
  const [scrolled, setScrolled] = useState(false)
  const [mobileOpen, setMobileOpen] = useState(false)
  const { scrollY } = useScroll()

  useMotionValueEvent(scrollY, 'change', (latest) => {
    setScrolled(latest > 48)
  })

  useEffect(() => {
    setMobileOpen(false)
  }, [pathname])

  return (
    <>
      <motion.header
        className={cn(
          'fixed top-0 left-0 right-0 z-50 transition-all duration-300',
          scrolled
            ? 'bg-bg/80 backdrop-blur-xl border-b border-white/[0.06]'
            : 'bg-transparent',
        )}
      >
        <nav className="mx-auto flex h-16 max-w-7xl items-center justify-between px-6 lg:px-8">
          {/* Logo */}
          <Link href="/" className="flex items-center gap-2" aria-label="Anthovai Home">
            <img src="/ANTHOVAI-BG.png" alt="Anthovai Logo" className="h-[60px] w-auto drop-shadow-md" onError={(e) => { e.currentTarget.style.display = 'none'; e.currentTarget.nextElementSibling?.classList.remove('hidden') }} />
            <span className="font-display text-xl tracking-tight text-white hidden">
              Anthovai
            </span>
          </Link>

          {/* Desktop Nav */}
          <div className="hidden lg:flex items-center gap-8">
            {navLinks.map((link) => (
              <Link
                key={link.key}
                href={link.href}
                className={cn(
                  'text-[11px] font-medium tracking-[0.15em] uppercase transition-colors',
                  pathname === link.href
                    ? 'text-gold'
                    : 'text-white-60 hover:text-white',
                )}
              >
                {t(link.key)}
              </Link>
            ))}
          </div>

          {/* Right side */}
          <div className="hidden lg:flex items-center gap-6">
            <LanguageSwitcher />
            <Link
              href="/contact"
              className={cn(
                'px-5 py-2 text-[11px] font-medium tracking-[0.15em] uppercase',
                'border border-gold-border text-gold hover:bg-gold-dim transition-all',
              )}
            >
              {t('contact')}
            </Link>
          </div>

          {/* Mobile toggle */}
          <button
            className="lg:hidden text-white p-2"
            onClick={() => setMobileOpen(!mobileOpen)}
            aria-label="Toggle navigation menu"
          >
            {mobileOpen ? <X size={20} /> : <Menu size={20} />}
          </button>
        </nav>
      </motion.header>

      {/* Mobile overlay */}
      <AnimatePresence>
        {mobileOpen && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-40 lg:hidden"
          >
            <div className="absolute inset-0 bg-bg/95 backdrop-blur-xl" />
            <div className="relative flex flex-col items-center justify-center h-full gap-8">
              {navLinks.map((link) => (
                <Link
                  key={link.key}
                  href={link.href}
                  className={cn(
                    'text-lg font-medium tracking-[0.1em] uppercase transition-colors',
                    pathname === link.href
                      ? 'text-gold'
                      : 'text-white-60 hover:text-white',
                  )}
                  onClick={() => setMobileOpen(false)}
                >
                  {t(link.key)}
                </Link>
              ))}
              <Link
                href="/contact"
                className="mt-4 px-8 py-3 text-sm font-medium tracking-[0.15em] uppercase border border-gold-border text-gold hover:bg-gold-dim transition-all"
                onClick={() => setMobileOpen(false)}
              >
                {t('contact')}
              </Link>
              <div className="mt-4">
                <LanguageSwitcher />
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </>
  )
}
