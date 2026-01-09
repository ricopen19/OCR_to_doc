import { Box } from '@mantine/core'
import { useEffect, useMemo, useRef, useState } from 'react'

import type { CropRect } from '../types/crop'

function clamp01(v: number) {
  return Math.max(0, Math.min(1, v))
}

function normalizeRect(a: { x: number; y: number }, b: { x: number; y: number }): CropRect {
  const left = clamp01(Math.min(a.x, b.x))
  const top = clamp01(Math.min(a.y, b.y))
  const right = clamp01(Math.max(a.x, b.x))
  const bottom = clamp01(Math.max(a.y, b.y))
  return { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) }
}

type ImageCropperProps = {
  src: string
  value?: CropRect
  onChange: (next?: CropRect) => void
}

export function ImageCropper({ src, value, onChange }: ImageCropperProps) {
  const imgRef = useRef<HTMLImageElement | null>(null)
  const dragLayerRef = useRef<HTMLDivElement | null>(null)
  const dragRef = useRef<{
    start: { x: number; y: number }
    current: { x: number; y: number }
  } | null>(null)
  const lastPointerRef = useRef<{ x: number; y: number } | null>(null)
  const scrollParentRef = useRef<HTMLElement | null>(null)
  const autoScrollRef = useRef<number | null>(null)
  const moveRafRef = useRef<number | null>(null)
  const pendingMoveRef = useRef<{ x: number; y: number } | null>(null)
  const [drag, setDrag] = useState<{
    start: { x: number; y: number }
    current: { x: number; y: number }
  } | null>(null)

  const setDragState = (next: typeof drag) => {
    dragRef.current = next
    setDrag(next)
  }

  const activeRect = useMemo(() => {
    if (drag) return normalizeRect(drag.start, drag.current)
    return value
  }, [drag, value])

  const rectStyle = useMemo(() => {
    if (!activeRect) return null
    return {
      left: `${activeRect.left * 100}%`,
      top: `${activeRect.top * 100}%`,
      width: `${activeRect.width * 100}%`,
      height: `${activeRect.height * 100}%`,
    } as const
  }, [activeRect])

  const toNormalizedPoint = (clientX: number, clientY: number) => {
    const img = imgRef.current
    if (!img) return null
    const rect = img.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) return null
    const x = clamp01((clientX - rect.left) / rect.width)
    const y = clamp01((clientY - rect.top) / rect.height)
    return { x, y }
  }

  const findScrollParent = (el: HTMLElement | null) => {
    let current = el
    while (current) {
      const style = window.getComputedStyle(current)
      const overflowY = style.overflowY
      if (
        (overflowY === 'auto' || overflowY === 'scroll') &&
        current.scrollHeight > current.clientHeight + 1
      ) {
        return current
      }
      current = current.parentElement
    }
    return (document.scrollingElement as HTMLElement | null) ?? null
  }

  const stopAutoScroll = () => {
    if (autoScrollRef.current != null) {
      cancelAnimationFrame(autoScrollRef.current)
      autoScrollRef.current = null
    }
  }

  const stopMoveRaf = () => {
    if (moveRafRef.current != null) {
      cancelAnimationFrame(moveRafRef.current)
      moveRafRef.current = null
    }
    pendingMoveRef.current = null
  }

  const scheduleDragMove = () => {
    if (moveRafRef.current != null) return
    moveRafRef.current = requestAnimationFrame(() => {
      moveRafRef.current = null
      const pending = pendingMoveRef.current
      const dragState = dragRef.current
      if (!pending || !dragState) return
      const p = toNormalizedPoint(pending.x, pending.y)
      if (!p) return
      setDragState({ start: dragState.start, current: p })
    })
  }

  const startAutoScroll = () => {
    if (autoScrollRef.current != null) return
    const threshold = 48
    const maxSpeed = 16
    const tick = () => {
      const dragState = dragRef.current
      const pointer = lastPointerRef.current
      const scrollParent = scrollParentRef.current
      if (!dragState || !pointer || !scrollParent) {
        stopAutoScroll()
        return
      }

      let top = 0
      let bottom = 0
      if (
        scrollParent === document.scrollingElement ||
        scrollParent === document.documentElement ||
        scrollParent === document.body
      ) {
        top = 0
        bottom = window.innerHeight
      } else {
        const rect = scrollParent.getBoundingClientRect()
        top = rect.top
        bottom = rect.bottom
      }

      const distanceTop = Math.max(0, threshold - (pointer.y - top))
      const distanceBottom = Math.max(0, threshold - (bottom - pointer.y))
      let delta = 0
      if (distanceTop > 0) {
        delta = -Math.ceil((distanceTop / threshold) * maxSpeed)
      } else if (distanceBottom > 0) {
        delta = Math.ceil((distanceBottom / threshold) * maxSpeed)
      }

      if (delta !== 0) {
        scrollParent.scrollBy({ top: delta })
        const p = toNormalizedPoint(pointer.x, pointer.y)
        if (p) {
          setDragState({ start: dragState.start, current: p })
        }
      }

      autoScrollRef.current = requestAnimationFrame(tick)
    }
    autoScrollRef.current = requestAnimationFrame(tick)
  }

  useEffect(() => {
    return () => {
      stopAutoScroll()
      stopMoveRaf()
    }
  }, [])

  return (
    <Box
      style={{
        position: 'relative',
        borderRadius: 'var(--mantine-radius-md)',
        overflow: 'hidden',
        background: 'var(--mantine-color-gray-0)',
        border: '1px solid var(--mantine-color-gray-3)',
      }}
    >
      <img
        ref={imgRef}
        src={src}
        alt="preview"
        style={{ width: '100%', height: 'auto', display: 'block', userSelect: 'none' }}
        draggable={false}
      />

      {/* Drag layer */}
      <Box
        ref={dragLayerRef}
        onPointerDown={(e) => {
          const p = toNormalizedPoint(e.clientX, e.clientY)
          if (!p) return
          e.currentTarget.setPointerCapture(e.pointerId)
          stopMoveRaf()
          lastPointerRef.current = { x: e.clientX, y: e.clientY }
          scrollParentRef.current = findScrollParent(dragLayerRef.current)
          setDragState({ start: p, current: p })
          onChange(undefined)
          startAutoScroll()
        }}
        onPointerMove={(e) => {
          if (!dragRef.current) return
          lastPointerRef.current = { x: e.clientX, y: e.clientY }
          pendingMoveRef.current = { x: e.clientX, y: e.clientY }
          scheduleDragMove()
        }}
        onPointerUp={(e) => {
          const dragState = dragRef.current
          if (!dragState) return
          try {
            const p = toNormalizedPoint(e.clientX, e.clientY)
            const current = p ?? dragState.current
            const rect = normalizeRect(dragState.start, current)
            if (rect.width > 0.005 && rect.height > 0.005) onChange(rect)
          } finally {
            setDragState(null)
            lastPointerRef.current = null
            scrollParentRef.current = null
            stopAutoScroll()
            stopMoveRaf()
            try {
              e.currentTarget.releasePointerCapture(e.pointerId)
            } catch {
              // ignore
            }
          }
        }}
        style={{
          position: 'absolute',
          inset: 0,
          cursor: 'crosshair',
          touchAction: 'none',
        }}
      />

      {rectStyle && (
        <>
          <Box
            style={{
              position: 'absolute',
              ...rectStyle,
              border: '2px solid var(--mantine-color-blue-6)',
              boxShadow: '0 0 0 9999px rgba(0,0,0,0.25)',
              pointerEvents: 'none',
            }}
          />
        </>
      )}
    </Box>
  )
}
