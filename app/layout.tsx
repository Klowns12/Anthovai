import type { Metadata } from 'next'
import '@/app/globals.css'

export const metadata: Metadata = {
  metadataBase: new URL('https://anthovai.com'),
  title: {
    default: 'Anthovai — Intelligence, Engineered.',
    template: '%s | Anthovai',
  },
  icons: {
    icon: '/ANTHOVAI-B.png',
  },
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return children
}
