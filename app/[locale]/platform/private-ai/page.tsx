import type { Metadata } from 'next'
import { getTranslations } from 'next-intl/server'
import { PrivateAiContent } from './PrivateAiContent'

type Props = { params: Promise<{ locale: string }> }

/**
 * The platform, installed inside a customer's own network.
 *
 * A server component only for the sake of `generateMetadata`: this page is
 * meant to be found by someone searching for exactly this, and `/platform`
 * — a client component throughout — cannot set its own title. The page body
 * is in `PrivateAiContent`, which needs the client for the same animation
 * primitives every other section uses.
 */
export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { locale } = await params
  const t = await getTranslations({ locale, namespace: 'private_ai_page' })

  return {
    title: t('meta_title'),
    description: t('meta_description'),
    alternates: { canonical: 'https://anthovai.com/platform/private-ai' },
  }
}

export default function PrivateAiPage() {
  return <PrivateAiContent />
}
