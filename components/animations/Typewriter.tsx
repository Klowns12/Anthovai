'use client'

import { useState, useEffect, useCallback } from 'react'
import { cn } from '@/lib/utils'

interface TypewriterProps {
  text: string
  speed?: number
  className?: string
  onComplete?: () => void
}

export function Typewriter({ text, speed = 40, className, onComplete }: TypewriterProps) {
  const [displayedText, setDisplayedText] = useState('')
  const [currentIndex, setCurrentIndex] = useState(0)

  const tick = useCallback(() => {
    if (currentIndex < text.length) {
      setDisplayedText(text.slice(0, currentIndex + 1))
      setCurrentIndex((i) => i + 1)
    } else {
      onComplete?.()
    }
  }, [currentIndex, text, onComplete])

  useEffect(() => {
    const timer = setTimeout(tick, speed)
    return () => clearTimeout(timer)
  }, [tick, speed])

  return (
    <span className={cn(className)}>
      {displayedText}
      <span className="animate-pulse text-gold">|</span>
    </span>
  )
}
