'use client'

import { useRef, useEffect } from 'react'
import * as THREE from 'three'

interface GlobeProps {
  className?: string
}

export function Globe({ className }: GlobeProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const frameRef = useRef<number>(0)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    const scene = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 100)
    camera.position.z = 4.5

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    container.appendChild(renderer.domElement)

    // Match Anthropic's precise faint thin lines
    const gridMaterial = new THREE.LineBasicMaterial({
      color: new THREE.Color('#4A4A4A'), // Charcoal faint
      transparent: true,
      opacity: 0.18,
    })

    const group = new THREE.Group()
    const radius = 2.0
    const latLines = 18
    const lonLines = 36

    // Longitude lines
    for (let i = 0; i < lonLines; i++) {
      const geometry = new THREE.EdgesGeometry(new THREE.CircleGeometry(radius, 64))
      const line = new THREE.LineLoop(geometry, gridMaterial)
      line.rotation.y = (i * Math.PI) / lonLines
      group.add(line)
    }

    // Latitude lines
    for (let i = 1; i < latLines; i++) {
      const phi = (i * Math.PI) / latLines
      const r = radius * Math.sin(phi)
      const y = radius * Math.cos(phi)
      
      const geometry = new THREE.EdgesGeometry(new THREE.CircleGeometry(r, 64))
      const line = new THREE.LineLoop(geometry, gridMaterial)
      line.rotation.x = Math.PI / 2
      line.position.y = y
      group.add(line)
    }
    
    // Plexus World Map Generation
    const plexusGroup = new THREE.Group()
    group.add(plexusGroup)

    const generatePlexus = async () => {
      try {
        // Fetch a specular map of the earth where water is light and land is dark
        const response = await fetch('https://raw.githubusercontent.com/mrdoob/three.js/master/examples/textures/planets/earth_specular_2048.jpg')
        if (!response.ok) return
        const blob = await response.blob()
        const img = new Image()
        img.src = URL.createObjectURL(blob)
        await new Promise((resolve) => { img.onload = resolve })

        const canvas = document.createElement('canvas')
        const imgWidth = 512 // Scale down for fast processing
        const imgHeight = 256
        canvas.width = imgWidth
        canvas.height = imgHeight
        const ctx = canvas.getContext('2d')
        if (!ctx) return
        ctx.drawImage(img, 0, 0, imgWidth, imgHeight)
        const imgData = ctx.getImageData(0, 0, imgWidth, imgHeight)
        const data = imgData.data

        const points: THREE.Vector3[] = []
        const samples = 12000 // Number of candidate points to check
        const goldenRatio = 1.618033988749895

        for (let i = 0; i < samples; i++) {
          const t = i / samples
          const inclination = Math.acos(1 - 2 * t)
          const azimuth = 2 * Math.PI * goldenRatio * i

          const x = Math.sin(inclination) * Math.cos(azimuth)
          const z = Math.sin(inclination) * Math.sin(azimuth)
          const y = Math.cos(inclination)

          // Convert 3D spherical coordinate to 2D UV coordinate
          const u = 0.5 + Math.atan2(z, x) / (2 * Math.PI)
          const v = 0.5 - Math.asin(y) / Math.PI

          // Sample pixels
          const pX = Math.floor(u * imgWidth)
          const pY = Math.floor(v * imgHeight)
          const index = (pY * imgWidth + pX) * 4
          
          // Specular map: water is bright (>128), land is dark (<128)
          // Handle strict TS array index possibly undefined
          const pixelValue = data[index]
          const isLand = pixelValue !== undefined ? pixelValue < 128 : false
          
          if (isLand && Math.random() > 0.2) { // 80% chance to keep to create texture
             points.push(new THREE.Vector3(x * radius, y * radius, z * radius))
          }
        }

        // 1. Create Dots for the Continents
        const dotGeo = new THREE.BufferGeometry().setFromPoints(points)
        const dotMat = new THREE.PointsMaterial({
          color: '#1A1A1A', // Dark dots (matches theme)
          size: 0.025,
          transparent: true,
          opacity: 0.9
        })
        const pointCloud = new THREE.Points(dotGeo, dotMat)
        plexusGroup.add(pointCloud)

        // 2. Create Interconnecting Network Lines (Plexus)
        const linePositions: number[] = []
        const threshold = 0.12 // Distance threshold for connection
        
        for (let i = 0; i < points.length; i++) {
          let connections = 0
          for (let j = i + 1; j < points.length; j++) {
            const p1 = points[i]
            const p2 = points[j]
            if (p1 && p2 && p1.distanceTo(p2) < threshold) {
              linePositions.push(
                p1.x, p1.y, p1.z,
                p2.x, p2.y, p2.z
              )
              connections++
              if (connections >= 3) break // Keep it abstract and sparse
            }
          }
        }
        
        const lineGeo = new THREE.BufferGeometry()
        lineGeo.setAttribute('position', new THREE.Float32BufferAttribute(linePositions, 3))
        const lineMat = new THREE.LineBasicMaterial({
          color: '#4A4A4A',
          transparent: true,
          opacity: 0.35, // Clear but subtle network lines
        })
        const plexusLines = new THREE.LineSegments(lineGeo, lineMat)
        plexusGroup.add(plexusLines)

      } catch (err) {
        console.error('Failed to generate plexus map', err)
      }
    }
    generatePlexus()

    // Subtle tilt to match Anthropic's visual angle
    group.rotation.x = 0.2
    scene.add(group)

    function resize() {
      if (!container) return
      const width = container.clientWidth
      const height = container.clientHeight
      camera.aspect = width / height
      camera.updateProjectionMatrix()
      renderer.setSize(width, height)
    }
    resize()
    window.addEventListener('resize', resize)

    // Interaction State
    let isDragging = false
    let previousMousePosition = { x: 0, y: 0 }
    let targetRotation = { x: 0.2, y: 0 }
    let autoRotationY = 0

    const onPointerDown = (e: PointerEvent) => {
      isDragging = true
      previousMousePosition = { x: e.clientX, y: e.clientY }
      container.style.cursor = 'grabbing'
    }

    const onPointerUp = () => {
      isDragging = false
      if (container) container.style.cursor = 'grab'
    }

    const onPointerMove = (e: PointerEvent) => {
      if (isDragging) {
        const deltaMove = {
          x: e.clientX - previousMousePosition.x,
          y: e.clientY - previousMousePosition.y
        }
        targetRotation.y += deltaMove.x * 0.005
        targetRotation.x += deltaMove.y * 0.005
        // Clamp X rotation to prevent flipping
        targetRotation.x = Math.max(-Math.PI / 2, Math.min(Math.PI / 2, targetRotation.x))
        previousMousePosition = { x: e.clientX, y: e.clientY }
      }
    }

    container.addEventListener('pointerdown', onPointerDown)
    window.addEventListener('pointerup', onPointerUp)
    window.addEventListener('pointermove', onPointerMove)
    container.style.cursor = 'grab'

    function animate() {
      frameRef.current = requestAnimationFrame(animate)

      if (!prefersReducedMotion) {
        if (!isDragging) {
          autoRotationY += 0.001
        }
        // Smoothly interpolate to target rotation + auto rotation
        group.rotation.y += ((targetRotation.y + autoRotationY) - group.rotation.y) * 0.05
        group.rotation.x += (targetRotation.x - group.rotation.x) * 0.1
      } else {
         group.rotation.y = targetRotation.y
         group.rotation.x = targetRotation.x
      }

      renderer.render(scene, camera)
    }
    animate()

    return () => {
      cancelAnimationFrame(frameRef.current)
      window.removeEventListener('resize', resize)
      container.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('pointerup', onPointerUp)
      window.removeEventListener('pointermove', onPointerMove)
      renderer.dispose()
      group.children.forEach((child: any) => {
        child.geometry?.dispose()
        child.material?.dispose()
      })
      if (container.contains(renderer.domElement)) {
        container.removeChild(renderer.domElement)
      }
    }
  }, [])

  return (
    <div
      ref={containerRef}
      className={className}
      style={{ width: '100%', height: '100%', touchAction: 'none' }}
      aria-hidden="true"
    />
  )
}
