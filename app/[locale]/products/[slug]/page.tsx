import { useTranslations } from 'next-intl'
import { FadeUp } from '@/components/animations/FadeUp'
import { notFound } from 'next/navigation'

export default async function ProductDetailPage({
  params,
}: {
  params: Promise<{ slug: string }>
}) {
  const { slug } = await params
  
  if (!['arkai', 'aello', 'alfa', 'klownsnexus'].includes(slug)) {
    notFound()
  }

  // Next-intl cannot do dynamic hooks at the root easily for metadata 
  // without a structured payload. We'll inline component usage.
  
  return <ProductContent slug={slug} />
}

function ProductContent({ slug }: { slug: string }) {
  const t = useTranslations('products')
  
  return (
    <>
      <main className="pt-32 pb-24 border-b border-white/[0.04]">
        {/* Hero */}
        <section className="py-32 relative overflow-hidden">
          <div className="absolute inset-0 bg-radial-gradient from-gold/[0.04] to-transparent pointer-events-none" />
          <div className="mx-auto max-w-7xl px-6 lg:px-8 relative z-10 text-center flex flex-col items-center">
            <FadeUp>
              <div className="inline-flex items-center px-4 py-1.5 text-xs font-medium tracking-[0.2em] uppercase border border-gold-border text-gold bg-gold-dim rounded-sm mb-8">
                {t(`${slug}.tag`)}
              </div>
              <h1 className="font-display text-[clamp(56px,7vw,96px)] leading-[1.05] tracking-[-0.02em] text-white max-w-4xl mb-8">
                {t(`${slug}.name`)}
              </h1>
              <p className="text-xl text-white-60 leading-relaxed max-w-2xl mx-auto">
                {t(`${slug}.desc`)}
              </p>
            </FadeUp>
          </div>
        </section>

        {/* Abstract Data Visualization Placeholder */}
        <section className="py-12">
          <div className="mx-auto max-w-6xl px-6 lg:px-8">
            <FadeUp delay={0.2}>
              <div className="aspect-video w-full bg-bg-2 border border-white/[0.06] rounded-xl overflow-hidden relative flex items-center justify-center">
                <div className="absolute inset-0 dot-grid opacity-30" />
                <div className="w-32 h-32 rounded-full border-[2px] border-gold/[0.2] animate-ping" />
                <div className="absolute w-16 h-16 rounded-full border border-gold/[0.4] animate-spin" style={{ animationDuration: '4s' }} />
                <div className="text-white-30 font-mono text-sm tracking-widest mt-40">
                  // {t(`${slug}.name`).toUpperCase()} INTERFACE PREVIEW //
                </div>
              </div>
            </FadeUp>
          </div>
        </section>
      </main>
    </>
  )
}
