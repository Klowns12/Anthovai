import { useTranslations } from 'next-intl'
import { Hero } from '@/components/sections/Hero'
import { About } from '@/components/sections/About'
import { PlatformSection } from '@/components/sections/PlatformSection'
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
      <TechStack />
      <Marquee items={marqueeItems} />
      <About />
      {/* After About and before Services: a visitor learns who we are, then
          the one thing they can start on their own today, then the work we do
          alongside them. */}
      <PlatformSection />
      <Services />
      <Process />
      <Manifesto />
      <ClientLogos />
      <FAQ />
    </>
  )
}
