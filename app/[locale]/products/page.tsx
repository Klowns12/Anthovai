import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { Link } from '@/i18n/navigation'
import { CTABand } from '@/components/sections/CTABand'
import { ArrowUpRight } from 'lucide-react'

export default function ProductsPage() {
  const t = useTranslations('products_page')
  const pt = useTranslations('products')

  const products = [
    { key: 'arkai', number: '01', href: '/products/arkai' },
    { key: 'aello', number: '02', href: '/products/aello' },
    { key: 'alfa', number: '03', href: '/products/alfa' },
    { key: 'klownsnexus', number: '04', href: '/products/klownsnexus' },
  ] as const

  return (
    <>
      <main className="pt-32 pb-24 border-b border-white/[0.04]">
        {/* Hero */}
        <section className="py-24 relative overflow-hidden border-b border-white/[0.04]">
          <div className="absolute inset-0 dot-grid opacity-30 pointer-events-none" />
          <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10">
            <FadeUp>
              <div className="flex items-center gap-3 mb-8">
                <span className="w-8 h-[1px] bg-gold" />
                <h1 className="text-[11px] font-medium tracking-[0.25em] uppercase text-gold">
                  {t('hero_label')}
                </h1>
              </div>
              <h2 className="font-display text-[clamp(48px,6vw,80px)] leading-[1.05] tracking-[-0.01em] text-white max-w-4xl mb-8">
                {t('hero_headline')}
              </h2>
              <p className="text-xl text-white-60 leading-relaxed max-w-2xl">
                {t('hero_body')}
              </p>
            </FadeUp>
          </div>
        </section>

        {/* Product Grid */}
        <section className="py-24 bg-bg-2">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-8 gap-y-16">
              {products.map((p, i) => (
                <FadeUp key={p.key} delay={i * 0.1}>
                  <Link href={p.href} className="group block h-full">
                    {/* Visual Placeholder */}
                    <div className="aspect-[4/3] bg-bg w-full rounded-t-xl border border-white/[0.06] border-b-0 relative overflow-hidden flex items-center justify-center">
                      <div className="absolute inset-0 bg-gradient-to-br from-bg-3 to-bg opacity-50" />
                      <span className="relative z-10 font-display italic text-6xl text-white/[0.03] group-hover:text-gold/[0.05] transition-colors duration-500 select-none">
                        {pt(`${p.key}.name`)}
                      </span>
                      {/* Glow on hover */}
                      <div className="absolute -bottom-1/2 left-1/2 -translate-x-1/2 w-[150%] aspect-square bg-radial-gradient from-gold/[0.03] to-transparent rounded-full opacity-0 group-hover:opacity-100 transition-opacity duration-700" />
                    </div>
                    
                    {/* Content */}
                    <div className="bg-bg border border-white/[0.06] p-8 lg:p-10 rounded-b-xl relative overflow-hidden group-hover:border-gold-border transition-colors duration-500">
                      <div className="flex justify-between items-start mb-6">
                        <span className="inline-flex items-center px-3 py-1 text-[10px] font-medium tracking-[0.2em] uppercase border border-gold-border text-gold bg-gold-dim rounded-sm">
                          {pt(`${p.key}.tag`)}
                        </span>
                        <div className="w-10 h-10 rounded-full border border-white/[0.08] flex items-center justify-center text-white-30 group-hover:bg-gold group-hover:text-bg group-hover:border-gold transition-all duration-300">
                          <ArrowUpRight size={18} />
                        </div>
                      </div>
                      <h3 className="font-display text-4xl text-white mb-4">
                        {pt(`${p.key}.name`)}
                      </h3>
                      <p className="text-white-60 leading-relaxed">
                        {pt(`${p.key}.desc`)}
                      </p>
                      
                      {/* Interactive bottom bar */}
                      <div className="absolute bottom-0 left-0 w-full h-[2px] bg-gold scale-x-0 origin-left group-hover:scale-x-100 transition-transform duration-500 ease-out" />
                    </div>
                  </Link>
                </FadeUp>
              ))}
            </div>
          </div>
        </section>
      </main>
      <CTABand />
    </>
  )
}
