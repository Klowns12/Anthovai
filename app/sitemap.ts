import { MetadataRoute } from 'next'

export default function sitemap(): MetadataRoute.Sitemap {
  const baseUrl = 'https://www.anthovai.com' // Update this to anthovai.com once domains are migrated

  // List of all non-dynamic canonical routes in the project
  const routes = [
    '',
    '/about',
    '/careers',
    '/contact',
    '/products',
    '/solutions',
    '/klang',
    '/klang/docs',
    '/expertise/enterprise-software',
    '/expertise/enterprise-software/healthcare',
    '/expertise/enterprise-software/retail',
    '/expertise/enterprise-software/manufacturing',
    '/expertise/ai-machine-learning'
  ]

  // Supported locales defined in next-intl config
  const locales = ['en', 'th']

  const sitemapEntries: MetadataRoute.Sitemap = []

  // Generate localized URLs for every route
  routes.forEach((route) => {
    locales.forEach((locale) => {
      sitemapEntries.push({
        url: `${baseUrl}/${locale}${route}`,
        lastModified: new Date(),
        changeFrequency: 'weekly',
        priority: route === '' ? 1 : 0.8,
      })
    })
  })

  // Add the absolute root domain which redirects to a locale
  sitemapEntries.unshift({
    url: baseUrl,
    lastModified: new Date(),
    changeFrequency: 'weekly',
    priority: 1,
  })

  return sitemapEntries
}
