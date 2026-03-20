'use client'

import { useTranslations } from 'next-intl'
import { motion } from 'framer-motion'
import { ArrowUpRight } from 'lucide-react'
import { Link } from '@/i18n/navigation'
import { Badge } from '../ui/Badge'
import { FadeUp } from '../animations/FadeUp'
import { StaggerChildren, staggerItem } from '../animations/StaggerChildren'

interface ProjectCardProps {
  number: string
  tag: string
  name: string
  desc: string
  href: string
}

function ProductCard({ number, tag, name, desc, href }: ProjectCardProps) {
  return (
    <Link href={href} className="group block h-full">
      <motion.div
        whileHover={{ scale: 0.98 }}
        transition={{ type: 'spring', stiffness: 300, damping: 30 }}
        className="relative h-full bg-bg-2 border border-white/[0.06] p-8 lg:p-10 overflow-hidden rounded-lg group-hover:border-gold-border group-hover:shadow-gold transition-colors duration-500 flex flex-col"
      >
        {/* Number watermark */}
        <div className="absolute -top-4 -right-4 text-[120px] font-display italic leading-none font-bold text-white/[0.02] group-hover:text-gold/[0.04] transition-colors duration-500 pointer-events-none select-none">
          {number}
        </div>

        {/* Content */}
        <div className="relative z-10 flex-1 flex flex-col">
          <div className="flex justify-between items-start mb-12">
            <Badge>{tag}</Badge>
            <div className="w-10 h-10 rounded-full border border-white/[0.08] flex items-center justify-center text-white-30 group-hover:bg-gold group-hover:text-bg group-hover:border-gold transition-all duration-300">
              <ArrowUpRight size={18} />
            </div>
          </div>
          
          <div className="mt-auto">
            <h3 className="font-display text-4xl lg:text-5xl text-white mb-4 tracking-[-0.01em]">
              {name}
            </h3>
            <p className="text-white-60 leading-relaxed max-w-sm">
              {desc}
            </p>
          </div>
        </div>

        {/* Bottom gold bar */}
        <div className="absolute bottom-0 left-0 w-full h-[2px] bg-gold scale-x-0 origin-left group-hover:scale-x-100 transition-transform duration-500 ease-out" />
      </motion.div>
    </Link>
  )
}

export function Products() {
  const t = useTranslations('products')

  const products = [
    { key: 'arkai', number: '01', href: '/products/arkai' },
    { key: 'aello', number: '02', href: '/products/aello' },
    { key: 'alfa', number: '03', href: '/products/alfa' },
    { key: 'klownsnexus', number: '04', href: '/products/klownsnexus' },
  ] as const

  return (
    <section className="py-32 relative">
      <div className="mx-auto max-w-7xl px-6 lg:px-8">
        <FadeUp>
          <div className="flex flex-col md:flex-row md:items-end justify-between gap-8 mb-16">
            <div>
              <div className="flex items-center gap-3 mb-6">
                <span className="w-8 h-[1px] bg-gold" />
                <h2 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('label')}
                </h2>
              </div>
              <h3 className="font-display text-[clamp(40px,5vw,64px)] leading-[1.05] tracking-[-0.01em] text-white">
                {t('headline')}
              </h3>
            </div>
            
            <Link 
              href="/products" 
              className="group flex items-center gap-2 text-sm font-medium tracking-[0.15em] uppercase text-white-60 hover:text-gold transition-colors"
            >
              {t('view_all')}
              <ArrowUpRight size={16} className="transition-transform group-hover:translate-x-1 group-hover:-translate-y-1" />
            </Link>
          </div>
        </FadeUp>

        <StaggerChildren className="grid grid-cols-1 md:grid-cols-2 gap-6 lg:gap-8">
          {products.map((p) => (
            <motion.div key={p.key} variants={staggerItem} className="h-full">
              <ProductCard
                number={p.number}
                tag={t(`${p.key}.tag`)}
                name={t(`${p.key}.name`)}
                desc={t(`${p.key}.desc`)}
                href={p.href}
              />
            </motion.div>
          ))}
        </StaggerChildren>
      </div>
    </section>
  )
}
