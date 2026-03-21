import { useTranslations } from 'next-intl'
import { getTranslations } from 'next-intl/server'
import { Link } from '@/i18n/navigation'
import { FadeUp } from '@/components/animations/FadeUp'
import { notFound } from 'next/navigation'

const validIndustries = ['healthcare', 'retail', 'manufacturing'] as const
type Industry = typeof validIndustries[number]

export async function generateMetadata({ params: { locale, industry } }: { params: { locale: string; industry: string } }) {
  if (!validIndustries.includes(industry as Industry)) return {}
  const t = await getTranslations({ locale, namespace: `expertise.enterpriseSoftware.${industry}.meta` })
  return {
    title: t('title'),
    description: t('description'),
  }
}

export default function IndustrySupportingPage({ params: { industry } }: { params: { industry: string } }) {
  if (!validIndustries.includes(industry as Industry)) {
    notFound()
  }

  const t = useTranslations(`expertise.enterpriseSoftware.${industry}`)
  
  // Sibling Map Logic
  const siblingMap: Record<Industry, { next: Industry, labelKey: string }> = {
    healthcare: { next: 'retail', labelKey: 'retail' },
    retail: { next: 'manufacturing', labelKey: 'manufacturing' },
    manufacturing: { next: 'healthcare', labelKey: 'healthcare' }
  }
  
  const currentIndustry = industry as Industry
  const sibling = siblingMap[currentIndustry]
  const tGlobal = useTranslations('expertise.enterpriseSoftware.industries')

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
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(faqSchema) }} />

      <article className="pt-32 pb-24">
        {/* Breadcrumb Navigation */}
        <div className="mx-auto max-w-7xl px-6 lg:px-8 mb-12">
          <nav aria-label="breadcrumb" className="text-sm font-medium text-text/60">
            <ol className="flex items-center flex-wrap space-x-2">
              <li><Link href="/" className="hover:text-text transition-colors">Home</Link></li>
              <li><span className="mx-2">→</span></li>
              <li><span className="cursor-default">Expertise</span></li>
              <li><span className="mx-2">→</span></li>
              <li><Link href="/expertise/enterprise-software" className="hover:text-text transition-colors">Enterprise Software</Link></li>
              <li><span className="mx-2">→</span></li>
              <li className="text-text capitalize" aria-current="page">{tGlobal(currentIndustry)}</li>
            </ol>
          </nav>
        </div>

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

        {/* Dynamic Sections */}
        <section className="mx-auto max-w-7xl px-6 lg:px-8 mb-24" id="features">
          <ul className="grid grid-cols-1 md:grid-cols-3 gap-12 border-t border-text/10 pt-16">
            {['s1', 's2', 's3'].map((sectionKey) => (
              <li key={sectionKey}>
                <FadeUp>
                  <h2 className="font-medium text-2xl text-text mb-4">{t(`sections.${sectionKey}.title`)}</h2>
                  <p className="text-text/70 leading-relaxed">{t(`sections.${sectionKey}.desc`)}</p>
                </FadeUp>
              </li>
            ))}
          </ul>
        </section>

        {/* FAQ Section */}
        <section className="mx-auto max-w-4xl px-6 lg:px-8 mb-24" id="faq">
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

        {/* Internal Sibling Linking */}
        <section className="mx-auto max-w-7xl px-6 lg:px-8 border-t border-text/10 pt-16" id="related">
          <FadeUp>
            <div className="flex flex-col md:flex-row justify-between items-center bg-bg-alt p-8 rounded-lg">
              <div>
                <h3 className="text-sm font-medium tracking-wide uppercase text-text/50 mb-2">Explore Related Industries</h3>
                <p className="text-2xl font-display text-text">{tGlobal(sibling.labelKey)}</p>
              </div>
              <Link 
                href={`/expertise/enterprise-software/${sibling.next}`} 
                className="mt-6 md:mt-0 flex items-center space-x-2 text-gold hover:text-gold/80 transition-colors font-medium border border-gold/20 px-6 py-3 rounded-full hover:bg-gold/5"
              >
                <span>Read More</span>
                <span>→</span>
              </Link>
            </div>
          </FadeUp>
        </section>

      </article>
    </main>
  )
}
