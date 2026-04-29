import { useTranslations } from 'next-intl'
import { Hero } from '@/components/sections/Hero'
import { About } from '@/components/sections/About'
import { Products } from '@/components/sections/Products'
import { KlangSection } from '@/components/sections/KlangSection'
import { Process } from '@/components/sections/Process'
import { ClientLogos } from '@/components/sections/ClientLogos'
import { Services } from '@/components/sections/Services'
import { Manifesto } from '@/components/sections/Manifesto'
import { FAQ } from '@/components/sections/FAQ'
import { Marquee } from '@/components/ui/Marquee'
import { TechStack } from '@/components/sections/TechStack'

export default function Home() {
  const t = useTranslations('marquee')
  const marqueeItems = t.raw('items') as string[]

  return (
    <>
      <Hero />
      <ClientLogos />
      {/* <TechStack /> */}
      <Marquee items={marqueeItems} />
      <About />
      <Services />
      <Process />
      <Manifesto />
      <FAQ />
    </>
  )
}
