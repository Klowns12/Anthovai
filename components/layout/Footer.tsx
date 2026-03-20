import { useTranslations } from 'next-intl'
import { Link } from '@/i18n/navigation'

const footerLinks = {
  platform: [
    { label: 'Arkai', href: '/products/arkai' },
    { label: 'Aello', href: '/products/aello' },
    { label: 'Alfa', href: '/products/alfa' },
    { label: 'KlownsNexus', href: '/products/klownsnexus' },
    { label: 'Klang', href: '/klang' },
  ],
  company: [
    { label: 'about', href: '/about' },
    { label: 'careers', href: '/careers' },
    { label: 'contact', href: '/contact' },
  ],
}

export function Footer() {
  const t = useTranslations('footer')
  const navT = useTranslations('nav')

  return (
    <footer className="border-t border-white/[0.06] bg-bg">
      <div className="mx-auto max-w-7xl px-6 lg:px-8 py-16">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-12">
          {/* Brand */}
          <div className="md:col-span-1">
            <Link href="/" className="font-display text-xl tracking-tight text-white">
              Anthovai
            </Link>
            <p className="mt-3 text-sm text-white-30 font-display italic">
              {t('tagline')}
            </p>
            <p className="mt-2 text-[11px] tracking-[0.15em] uppercase text-white-30">
              {t('parent')}
            </p>
          </div>

          {/* Platform */}
          <div>
            <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-4">
              {t('platform')}
            </h3>
            <ul className="space-y-3">
              {footerLinks.platform.map((link) => (
                <li key={link.href}>
                  <Link
                    href={link.href}
                    className="text-sm text-white-60 hover:text-white transition-colors"
                  >
                    {link.label}
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          {/* Company */}
          <div>
            <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-4">
              {t('company')}
            </h3>
            <ul className="space-y-3">
              {footerLinks.company.map((link) => (
                <li key={link.href}>
                  <Link
                    href={link.href}
                    className="text-sm text-white-60 hover:text-white transition-colors"
                  >
                    {navT(link.label)}
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          {/* Connect */}
          <div>
            <h3 className="text-[11px] font-medium tracking-[0.2em] uppercase text-white-30 mb-4">
              {t('connect')}
            </h3>
            <ul className="space-y-3">
              <li>
                <a
                  href="mailto:hello@anthovai.com"
                  className="text-sm text-white-60 hover:text-white transition-colors"
                >
                  hello@anthovai.com
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/anthovai"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-sm text-white-60 hover:text-white transition-colors"
                >
                  GitHub
                </a>
              </li>
              <li>
                <a
                  href="https://linkedin.com/company/anthovai"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-sm text-white-60 hover:text-white transition-colors"
                >
                  LinkedIn
                </a>
              </li>
            </ul>
          </div>
        </div>

        {/* Bottom */}
        <div className="mt-16 pt-8 border-t border-white/[0.06]">
          <p className="text-xs text-white-30 text-center">
            {t('copy')}
          </p>
        </div>
      </div>
    </footer>
  )
}
