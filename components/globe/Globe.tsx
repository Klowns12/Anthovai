'use client'

import { useRef, useEffect, useMemo } from 'react'
import * as THREE from 'three'

interface GlobeProps {
  className?: string
}

// Fibonacci sphere point distribution
function fibonacciSphere(count: number): THREE.Vector3[] {
  const points: THREE.Vector3[] = []
  const goldenRatio = (1 + Math.sqrt(5)) / 2

  for (let i = 0; i < count; i++) {
    const theta = (2 * Math.PI * i) / goldenRatio
    const phi = Math.acos(1 - (2 * (i + 0.5)) / count)
    const x = Math.cos(theta) * Math.sin(phi)
    const y = Math.cos(phi)
    const z = Math.sin(theta) * Math.sin(phi)
    points.push(new THREE.Vector3(x, y, z))
  }

  return points
}

export function Globe({ className }: GlobeProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const mouseRef = useRef({ x: 0, y: 0 })
  const frameRef = useRef<number>(0)

  const dotPositions = useMemo(() => fibonacciSphere(500), [])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return

    // Check reduced motion preference
    const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    // Scene setup
    const scene = new THREE.Scene()
    const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 100)
    camera.position.z = 4.5

    const renderer = new THREE.WebGLRenderer({
      antialias: true,
      alpha: true,
    })
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    container.appendChild(renderer.domElement)

    // Wireframe sphere
    const sphereGeometry = new THREE.SphereGeometry(1.8, 48, 48)
    const wireframeMaterial = new THREE.MeshBasicMaterial({
      color: new THREE.Color('rgba(201,168,76,0.12)'),
      wireframe: true,
      transparent: true,
      opacity: 0.12,
    })
    const sphere = new THREE.Mesh(sphereGeometry, wireframeMaterial)
    scene.add(sphere)

    // Dots
    const dotsGeometry = new THREE.BufferGeometry()
    const positions = new Float32Array(dotPositions.length * 3)

    dotPositions.forEach((pos, i) => {
      positions[i * 3] = pos.x * 1.82
      positions[i * 3 + 1] = pos.y * 1.82
      positions[i * 3 + 2] = pos.z * 1.82
    })

    dotsGeometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))

    const dotsMaterial = new THREE.PointsMaterial({
      color: new THREE.Color('#C9A84C'),
      size: 0.02,
      transparent: true,
      opacity: 0.6,
      sizeAttenuation: true,
    })

    const dots = new THREE.Points(dotsGeometry, dotsMaterial)
    scene.add(dots)

    // Resize handler
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

    // Mouse handler
    function onMouseMove(e: MouseEvent) {
      if (!container) return
      const rect = container.getBoundingClientRect()
      mouseRef.current = {
        x: ((e.clientX - rect.left) / rect.width - 0.5) * 2,
        y: ((e.clientY - rect.top) / rect.height - 0.5) * 2,
      }
    }
    container.addEventListener('mousemove', onMouseMove)

    // Animation loop
    let rotationY = 0
    function animate() {
      frameRef.current = requestAnimationFrame(animate)

      if (!prefersReducedMotion) {
        rotationY += 0.0008
        sphere.rotation.y = rotationY
        dots.rotation.y = rotationY

        // Mouse parallax
        const targetRotX = mouseRef.current.y * 0.15
        const targetRotZ = mouseRef.current.x * 0.15
        sphere.rotation.x += (targetRotX - sphere.rotation.x) * 0.02
        dots.rotation.x = sphere.rotation.x
        sphere.rotation.z += (targetRotZ - sphere.rotation.z) * 0.02
        dots.rotation.z = sphere.rotation.z
      }

      renderer.render(scene, camera)
    }
    animate()

    // Cleanup
    return () => {
      cancelAnimationFrame(frameRef.current)
      window.removeEventListener('resize', resize)
      container.removeEventListener('mousemove', onMouseMove)
      renderer.dispose()
      sphereGeometry.dispose()
      wireframeMaterial.dispose()
      dotsGeometry.dispose()
      dotsMaterial.dispose()
      if (container.contains(renderer.domElement)) {
        container.removeChild(renderer.domElement)
      }
    }
  }, [dotPositions])

  return (
    <div
      ref={containerRef}
      className={className}
      style={{ width: '100%', height: '100%' }}
      aria-hidden="true"
    />
  )
}
