import { useTranslations } from 'next-intl'
import { getTranslations } from 'next-intl/server'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'

type Props = {
  params: Promise<{
    locale: string
  }>
}

export async function generateMetadata({ params }: Props) {
  const { locale } = await params
  const t = await getTranslations({ locale, namespace: 'expertise.enterpriseSoftware.meta' })
  return {
    title: t('title'),
    description: t('description'),
  }
}

export default function EnterpriseSoftwarePillar() {
  const t = useTranslations('expertise.enterpriseSoftware')

  const serviceSchema = {
    "@context": "https://schema.org",
    "@type": "Service",
    "name": "Enterprise Software Development",
    "provider": {
      "@type": "Organization",
      "name": "Anthovai",
      "url": "https://anthovai.com"
    },
    "areaServed": ["TH", "Worldwide"],
    "description": t('meta.description'),
    "hasOfferCatalog": {
      "@type": "OfferCatalog",
      "name": "Enterprise Software Services",
      "itemListElement": [
        { "@type": "Offer", "itemOffered": { "@type": "Service", "name": "Healthcare Software Development" }},
        { "@type": "Offer", "itemOffered": { "@type": "Service", "name": "Retail & E-commerce Software" }},
        { "@type": "Offer", "itemOffered": { "@type": "Service", "name": "Manufacturing Software & MES" }}
      ]
    }
  }

  const faqItems = ['q1', 'q2', 'q3', 'q4']
  const faqSchema = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    "mainEntity": faqItems.map((q) => ({
      "@type": "Question",
      "name": t(`faq.${q}.q`),
      "acceptedAnswer": {
        "@type": "Answer",
        "text": t(`faq.${q}.a`)
      }
    }))
  }

  return (
    <main className="min-h-screen bg-bg">
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(serviceSchema) }} />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(faqSchema) }} />

      <article className="pt-32 pb-24">
        {/* Answer-First Hero */}
        <section className="mx-auto max-w-7xl px-6 lg:px-8 mb-24" id="hero">
          <FadeUp>
            <h1 className="font-display text-4xl md:text-6xl tracking-tight text-text mb-6">
              {t('hero.h1')}
            </h1>
            <p className="text-xl md:text-2xl text-text/80 leading-relaxed max-w-4xl">
              {t('hero.intro')}
            </p>
          </FadeUp>
        </section>

        {/* What is Enterprise Software */}
        <section className="mx-auto max-w-4xl px-6 lg:px-8 mb-24" id="definition">
          <FadeUp>
            <h2 className="text-[11px] font-medium tracking-[0.15em] uppercase text-text/50 mb-4">{t('whatIs.title')}</h2>
            <p className="text-lg md:text-xl text-text/70">{t('whatIs.body')}</p>
          </FadeUp>
        </section>

        {/* Why Anthovai */}
        <section className="mx-auto max-w-7xl px-6 lg:px-8 mb-24" id="differentiators">
          <FadeUp>
            <h2 className="font-display text-3xl md:text-5xl text-text mb-12">{t('whyAnthovai.title')}</h2>
            <ul className="grid grid-cols-1 md:grid-cols-3 gap-12">
              {['p1', 'p2', 'p3'].map((p) => (
                <li key={p}>
                  <h3 className="font-medium text-xl text-text mb-3">{t(`whyAnthovai.points.${p}.title`)}</h3>
                  <p className="text-text/70">{t(`whyAnthovai.points.${p}.desc`)}</p>
                </li>
              ))}
            </ul>
          </FadeUp>
        </section>

        {/* Our Process (ol semantic list) */}
        <section className="bg-bg-alt py-24 mb-24" id="process">
          <div className="mx-auto max-w-7xl px-6 lg:px-8">
            <FadeUp>
              <h2 className="font-display text-3xl md:text-5xl text-text mb-12">{t('process.title')}</h2>
              <ol className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-8">
                {['s1', 's2', 's3', 's4'].map((s, i) => (
                  <li key={s} className="border-t border-text/10 pt-6">
                    <span className="text-text/40 font-display text-2xl mb-4 block">0{i + 1}</span>
                    <h3 className="font-medium text-xl text-text mb-3">{t(`process.steps.${s}.title`)}</h3>
                    <p className="text-text/70 text-sm">{t(`process.steps.${s}.desc`)}</p>
                  </li>
                ))}
              </ol>
            </FadeUp>
          </div>
        </section>

        {/* Industries We Serve (Linking to Supporting Pages) */}
        <section className="mx-auto max-w-7xl px-6 lg:px-8 mb-24" id="industries">
          <FadeUp>
            <h2 className="font-display text-3xl md:text-5xl text-text mb-12">{t('industries.title')}</h2>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
              <Link href="/expertise/enterprise-software/healthcare" className="group block p-8 border border-text/10 hover:border-gold transition-colors">
                <h3 className="text-xl font-medium text-text group-hover:text-gold transition-colors mb-4">{t('industries.healthcare')}</h3>
                <span className="text-sm tracking-wide uppercase text-text/50 group-hover:text-gold/80 transition-colors">{t('industries.readMore')} →</span>
              </Link>
              <Link href="/expertise/enterprise-software/retail" className="group block p-8 border border-text/10 hover:border-gold transition-colors">
                <h3 className="text-xl font-medium text-text group-hover:text-gold transition-colors mb-4">{t('industries.retail')}</h3>
                <span className="text-sm tracking-wide uppercase text-text/50 group-hover:text-gold/80 transition-colors">{t('industries.readMore')} →</span>
              </Link>
              <Link href="/expertise/enterprise-software/manufacturing" className="group block p-8 border border-text/10 hover:border-gold transition-colors">
                <h3 className="text-xl font-medium text-text group-hover:text-gold transition-colors mb-4">{t('industries.manufacturing')}</h3>
                <span className="text-sm tracking-wide uppercase text-text/50 group-hover:text-gold/80 transition-colors">{t('industries.readMore')} →</span>
              </Link>
            </div>

            <div className="mt-12 text-center border-t border-text/10 pt-12">
              <Link href="/expertise/ai-machine-learning" className="inline-flex items-center space-x-2 text-text/70 hover:text-gold hover:underline transition-colors">
                <span>View all cross-cluster expertise: AI & Machine Learning Infrastructure</span>
              </Link>
            </div>
          </FadeUp>
        </section>

        {/* FAQ Section */}
        <section className="mx-auto max-w-4xl px-6 lg:px-8" id="faq">
          <FadeUp>
            <h2 className="font-display text-3xl md:text-5xl text-text mb-12 text-center">{t('faq.title')}</h2>
            <div className="space-y-6">
              {faqItems.map((q) => (
                <div key={q} className="border-b border-text/10 pb-6">
                  <h3 className="font-medium text-lg text-text mb-2">{t(`faq.${q}.q`)}</h3>
                  <p className="text-text/70">{t(`faq.${q}.a`)}</p>
                </div>
              ))}
            </div>
          </FadeUp>
        </section>

      </article>
    </main>
  )
}
