'use client'

import { cn } from '@/lib/utils'

interface MarqueeProps {
  items: string[]
  className?: string
  speed?: number
}

export function Marquee({ items, className, speed = 30 }: MarqueeProps) {
  const content = items.join(' · ')

  return (
    <div
      className={cn(
        'relative overflow-hidden border-y border-white/[0.06] py-4',
        className
      )}
    >
      <div
        className="flex whitespace-nowrap"
        style={{
          animation: `marquee ${speed}s linear infinite`,
        }}
      >
        <span className="mx-4 text-xs font-medium tracking-[0.3em] uppercase text-white-30">
          {content} · {content} · {content} · {content}
        </span>
        <span className="mx-4 text-xs font-medium tracking-[0.3em] uppercase text-white-30">
          {content} · {content} · {content} · {content}
        </span>
      </div>
    </div>
  )
}
