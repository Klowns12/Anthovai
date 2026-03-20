'use client'

import dynamic from 'next/dynamic'
import { Suspense } from 'react'

const GlobeComponent = dynamic(() => import('./Globe').then((mod) => mod.Globe), {
  ssr: false,
  loading: () => (
    <div className="w-full h-full flex items-center justify-center relative">
      <div className="absolute inset-0 bg-gold-dim rounded-full animate-pulse-dot" />
      <div className="w-1/2 h-1/2 rounded-full border border-gold-border animate-spin" style={{ animationDuration: '3s' }} />
    </div>
  ),
})

export function GlobeCanvas({ className }: { className?: string }) {
  return (
    <div className={className}>
      <Suspense fallback={null}>
        <GlobeComponent />
      </Suspense>
    </div>
  )
}
