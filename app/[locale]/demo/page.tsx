import type { Metadata } from 'next'
import { getTranslations } from 'next-intl/server'
import { DeliveryNoteDemo } from '@/components/demo/DeliveryNoteDemo'

type Props = { params: Promise<{ locale: string }> }

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { locale } = await params
  const t = await getTranslations({ locale, namespace: 'demo' })

  return {
    title: t('meta_title'),
    description: t('meta_description'),
    alternates: { canonical: 'https://anthovai.com/demo' },
  }
}

export default function DemoPage() {
  return <DeliveryNoteDemo />
}
