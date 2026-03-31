import type { Metadata } from 'next'
import { NextIntlClientProvider, useMessages } from 'next-intl'
import { getTranslations } from 'next-intl/server'
import { notFound } from 'next/navigation'
import { routing } from '@/i18n/routing'
import { instrumentSerif, dmSans, jetbrainsMono } from '@/lib/fonts'
import { Navbar } from '@/components/layout/Navbar'
import { Footer } from '@/components/layout/Footer'
import '@/app/globals.css'

type Props = {
  children: React.ReactNode
  params: Promise<{ locale: string }>
}

export async function generateStaticParams() {
  return routing.locales.map((locale) => ({ locale }))
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { locale } = await params
  const t = await getTranslations({ locale, namespace: 'metadata' })

  return {
    metadataBase: new URL('https://anthovai.com'),
    title: {
      default: 'Anthovai — Intelligence, Engineered.',
      template: '%s | Anthovai',
    },
    description: t('description'),
    keywords: ['AI', 'Software', 'Infrastructure', 'Enterprise', 'Thailand', 'Klowns Language', 'Kaiser Klowns'],
    authors: [{ name: 'Anthovai', url: 'https://anthovai.com' }],
    openGraph: {
      type: 'website',
      url: 'https://anthovai.com',
      siteName: 'Anthovai',
      images: [{ url: 'https://anthovai.com/ANTHOVAI-W.png', width: 1200, height: 630 }],
    },
    twitter: {
      card: 'summary_large_image',
      images: ['https://anthovai.com/ANTHOVAI-W.png'],
    },
    robots: { index: true, follow: true },
    alternates: {
      canonical: 'https://anthovai.com',
      languages: Object.fromEntries(
        routing.locales.map((l) => [l, `https://anthovai.com/${l}`])
      ),
    },
  }
}

export default async function LocaleLayout({ children, params }: Props) {
  const { locale } = await params

  if (!routing.locales.includes(locale as (typeof routing.locales)[number])) {
    notFound()
  }

  const messages = (await import(`@/messages/${locale}.json`)).default

  return (
    <html lang={locale} suppressHydrationWarning>
      <body
        className={`${instrumentSerif.variable} ${dmSans.variable} ${jetbrainsMono.variable} antialiased`}
      >
        <NextIntlClientProvider locale={locale} messages={messages}>
          <Navbar />
          <main>{children}</main>
          <Footer />
        </NextIntlClientProvider>
      </body>
    </html>
  )
}
