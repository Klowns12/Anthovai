'use client'

import { useRef } from 'react'
import { useInView, useMotionValue, useTransform, animate } from 'framer-motion'
import { useEffect } from 'react'

interface CounterProps {
  from?: number
  to: number
  suffix?: string
  prefix?: string
  duration?: number
  className?: string
}

export function Counter({
  from = 0,
  to,
  suffix = '',
  prefix = '',
  duration = 2,
  className,
}: CounterProps) {
  const ref = useRef<HTMLSpanElement>(null)
  const isInView = useInView(ref, { once: true })
  const count = useMotionValue(from)
  const rounded = useTransform(count, (v) => Math.round(v))

  useEffect(() => {
    if (isInView) {
      const controls = animate(count, to, {
        duration,
        ease: 'easeOut',
      })
      return controls.stop
    }
  }, [isInView, count, to, duration])

  useEffect(() => {
    const unsubscribe = rounded.on('change', (v) => {
      if (ref.current) {
        ref.current.textContent = `${prefix}${v}${suffix}`
      }
    })
    return unsubscribe
  }, [rounded, suffix, prefix])

  return (
    <span ref={ref} className={className}>
      {prefix}{from}{suffix}
    </span>
  )
}
