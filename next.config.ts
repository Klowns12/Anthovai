import createNextIntlPlugin from 'next-intl/plugin'
import type { NextConfig } from 'next'

const withNextIntl = createNextIntlPlugin('./i18n/request.ts')

const config: NextConfig = {
  images: {
    formats: ['image/avif', 'image/webp'],
  },
  webpack: (config) => {
    config.externals = [...(config.externals as string[] || []), { canvas: 'canvas' }]
    return config
  },
}

export default withNextIntl(config)
