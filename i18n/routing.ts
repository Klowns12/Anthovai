import { defineRouting } from 'next-intl/routing'

export const routing = defineRouting({
  locales: ['en', 'th', 'zh', 'ja', 'fr', 'de', 'ko', 'es'],
  defaultLocale: 'en',
  localePrefix: 'never'
})
