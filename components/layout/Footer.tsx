'use client'

import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'
import { motion } from 'framer-motion'
import { ArrowUpRight, Mail, MapPin, Clock } from 'lucide-react'

const footerLinks = {
  company: [
    { label: 'about', href: '/about' },
    { label: 'careers', href: '/careers' },
    { label: 'contact', href: '/contact' },
  ],
}

const socialLinks = [
  { label: 'Facebook', href: 'https://www.facebook.com/anthovai25' },
  { label: 'LinkedIn', href: 'https://www.linkedin.com/showcase/anthovai/about/?viewAsMember=true' },
  { label: 'Instagram', href: 'https://www.instagram.com/anthovai?igsh=MWhua3B5aHcxbndmZg==' },
]

export function Footer() {
  const t = useTranslations('footer')
  const navT = useTranslations('nav')

  return (
    <footer className="relative bg-bg overflow-hidden border-t border-white/[0.06]">
      {/* Decorative background elements */}
      <div className="absolute inset-0 pointer-events-none">
        {/* Subtle grid */}
        <div className="absolute inset-0 opacity-[0.2]"
          style={{
            backgroundImage: 'linear-gradient(rgba(201,168,76,0.15) 1px, transparent 1px), linear-gradient(90deg, rgba(201,168,76,0.15) 1px, transparent 1px)',
            backgroundSize: '64px 64px',
          }}
        />
        {/* Radial glow */}
        <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[800px] h-[400px] bg-[radial-gradient(ellipse_at_center,rgba(201,168,76,0.08)_0%,transparent_70%)]" />
        {/* Bottom corner accent */}
        <div className="absolute bottom-0 right-0 w-[400px] h-[400px] bg-[radial-gradient(circle_at_bottom_right,rgba(201,168,76,0.05)_0%,transparent_60%)]" />
      </div>

      {/* Top gold line */}
      <motion.div
        className="h-[1px] bg-gradient-to-r from-transparent via-gold/30 to-transparent"
        initial={{ scaleX: 0 }}
        whileInView={{ scaleX: 1 }}
        viewport={{ once: true }}
        transition={{ duration: 1.2, ease: 'easeInOut' }}
      />

      {/* CTA Band */}
      <div className="relative mx-auto max-w-7xl px-6 lg:px-8 pt-20 pb-16">
        {/* <motion.div
          className="flex flex-col md:flex-row items-center justify-between gap-8 pb-16 border-b border-black/[0.06]"
          initial={{ opacity: 0, y: 30 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.7 }}
        >
          <div>
            <h3 className="font-display text-[clamp(28px,4vw,48px)] text-white tracking-tight leading-[1.1]">
              {ctaT('headline')}
            </h3>
            <p className="mt-3 text-white-60 text-base max-w-md">
              {ctaT('sub')}
            </p>
          </div>
          <Link
            href="/contact"
            className="group flex items-center gap-3 px-8 py-4 bg-gold-dim border border-gold-border text-gold hover:bg-gold/10 hover:border-gold/40 transition-all duration-300 text-sm tracking-[0.15em] uppercase font-medium bg-white"
          >
            {ctaT('button')}
            <ArrowUpRight size={16} className="transition-transform group-hover:translate-x-1 group-hover:-translate-y-1" />
          </Link>
        </motion.div> */}
      </div>

      {/* Main Footer Content */}
      <div className="relative mx-auto max-w-7xl px-6 lg:px-8 pb-16">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-12">
          {/* Brand */}
          <motion.div
            className="lg:col-span-1"
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5 }}
          >
            <Link href="/" aria-label="Anthovai Home">
              <img src="/ANTHOVAI-BG.png" alt="Anthovai Logo" className="h-[50px] w-auto drop-shadow-sm hover:scale-105 transition-transform duration-500 origin-left" />
            </Link>
            <p className="mt-5 text-sm text-white-60 font-display italic">
              {t('tagline')}
            </p>
            <p className="mt-2 text-[10px] tracking-[0.2em] uppercase text-white-30">
              {t('parent')}
            </p>
          </motion.div>

          {/* Company */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5, delay: 0.1 }}
          >
            <h3 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-5">
              {t('company')}
            </h3>
            <ul className="space-y-3">
              {footerLinks.company.map((link) => (
                <li key={link.href}>
                  <Link
                    href={link.href}
                    className="group text-sm text-white-60 hover:text-gold transition-colors flex items-center gap-2"
                  >
                    {navT(link.label)}
                    <ArrowUpRight size={12} className="opacity-0 -translate-x-2 group-hover:translate-x-0 group-hover:opacity-100 transition-all duration-300" />
                  </Link>
                </li>
              ))}
            </ul>
          </motion.div>

          {/* Connect */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5, delay: 0.2 }}
          >
            <h3 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-5">
              {t('connect')}
            </h3>
            <ul className="space-y-3">
              {socialLinks.map((link) => (
                <li key={link.href}>
                  <a
                    href={link.href}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="group text-sm text-white-60 hover:text-gold transition-colors flex items-center gap-2"
                  >
                    {link.label}
                    <ArrowUpRight size={12} className="opacity-0 -translate-x-2 group-hover:translate-x-0 group-hover:opacity-100 transition-all duration-300" />
                  </a>
                </li>
              ))}
            </ul>
          </motion.div>

          {/* Contact Info */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5, delay: 0.3 }}
          >
            <h3 className="text-[11px] font-medium tracking-[0.25em] uppercase text-white-30 mb-5">
              INFO
            </h3>
            <ul className="space-y-4">
              <li className="flex items-start gap-3 group">
                <div className="w-6 h-6 rounded-full bg-gold/10 flex items-center justify-center shrink-0 group-hover:bg-gold/20 transition-colors">
                  <Mail size={12} className="text-gold" />
                </div>
                <a href="mailto:contact@anthovai.com" className="text-sm text-white-60 hover:text-gold transition-colors pt-0.5">
                  contact@anthovai.com
                </a>
              </li>
              <li className="flex items-start gap-3 group">
                <div className="w-6 h-6 rounded-full bg-gold/10 flex items-center justify-center shrink-0 group-hover:bg-gold/20 transition-colors">
                  <MapPin size={12} className="text-gold" />
                </div>
                <span className="text-sm text-white-60 pt-0.5">Bangkok, Thailand</span>
              </li>
              <li className="flex items-start gap-3 group">
                <div className="w-6 h-6 rounded-full bg-gold/10 flex items-center justify-center shrink-0 group-hover:bg-gold/20 transition-colors">
                  <Clock size={12} className="text-gold" />
                </div>
                <span className="text-sm text-white-60 pt-0.5">Mon–Fri 9:00–18:00 ICT</span>
              </li>
            </ul>
            <div className="mt-6">
              <a href="https://lin.ee/8h2u2B9" target="_blank" rel="noopener noreferrer" className="inline-block hover:opacity-80 transition-opacity">
                <img src="https://scdn.line-apps.com/n/line_add_friends/btn/th.png" alt="เพิ่มเพื่อน" height="36" className="h-[36px] w-auto" />
              </a>
            </div>
          </motion.div>
        </div>
      </div>

      {/* Bottom bar */}
      <div className="relative border-t border-black/[0.04] bg-white/30 backdrop-blur-sm">
        <div className="mx-auto max-w-7xl px-6 lg:px-8 py-6 flex flex-col md:flex-row items-center justify-between gap-4">
          <p className="text-[11px] text-white-30 tracking-wide font-medium">
            {t('copy')}
          </p>
          <div className="flex items-center gap-6">
            {socialLinks.map((link) => (
              <a
                key={link.label}
                href={link.href}
                target="_blank"
                rel="noopener noreferrer"
                className="text-[10px] text-white-30 hover:text-gold tracking-[0.15em] uppercase transition-colors font-medium"
              >
                {link.label}
              </a>
            ))}
          </div>
        </div>
      </div>
    </footer>
  )
}
