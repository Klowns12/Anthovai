'use client'

import { useEffect, useState } from 'react'
import { motion, useMotionValue, useSpring } from 'framer-motion'
import { cn } from '@/lib/utils'

export function Cursor() {
  const [mounted, setMounted] = useState(false)
  const [isHovering, setIsHovering] = useState(false)
  
  // Mouse position
  const mouseX = useMotionValue(0)
  const mouseY = useMotionValue(0)

  // Smooth springs for the ring
  const ringX = useSpring(mouseX, { stiffness: 300, damping: 30 })
  const ringY = useSpring(mouseY, { stiffness: 300, damping: 30 })

  useEffect(() => {
    // Only show on non-touch devices
    if (window.matchMedia('(pointer: fine)').matches) {
      setMounted(true)
    }

    const updateMousePosition = (e: MouseEvent) => {
      mouseX.set(e.clientX)
      mouseY.set(e.clientY)
    }

    const handleMouseOver = (e: MouseEvent) => {
      const target = e.target as HTMLElement
      // Check if hovering over clickable elements
      if (
        window.getComputedStyle(target).cursor === 'pointer' ||
        target.tagName.toLowerCase() === 'a' ||
        target.tagName.toLowerCase() === 'button' ||
        target.closest('a') ||
        target.closest('button')
      ) {
        setIsHovering(true)
      } else {
        setIsHovering(false)
      }
    }

    window.addEventListener('mousemove', updateMousePosition)
    window.addEventListener('mouseover', handleMouseOver)

    return () => {
      window.removeEventListener('mousemove', updateMousePosition)
      window.removeEventListener('mouseover', handleMouseOver)
    }
  }, [mouseX, mouseY])

  if (!mounted) return null

  return (
    <>
      {/* Dot */}
      <motion.div
        className="fixed top-0 left-0 w-2 h-2 bg-gold rounded-full pointer-events-none z-[100] transform -translate-x-1/2 -translate-y-1/2 mix-blend-exclusion"
        style={{ x: mouseX, y: mouseY }}
      />
      
      {/* Ring */}
      <motion.div
        className={cn(
          'fixed top-0 left-0 rounded-full border border-gold pointer-events-none z-[100] transform -translate-x-1/2 -translate-y-1/2 transition-all duration-300 ease-out',
          isHovering ? 'w-14 h-14 bg-gold/[0.08] opacity-100' : 'w-9 h-9 opacity-60'
        )}
        style={{ x: ringX, y: ringY }}
      />
    </>
  )
}
