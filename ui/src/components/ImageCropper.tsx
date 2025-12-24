import { Box } from '@mantine/core'
import { useMemo, useRef, useState } from 'react'

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
  const [drag, setDrag] = useState<{
    start: { x: number; y: number }
    current: { x: number; y: number }
  } | null>(null)

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
        onPointerDown={(e) => {
          const p = toNormalizedPoint(e.clientX, e.clientY)
          if (!p) return
          e.currentTarget.setPointerCapture(e.pointerId)
          setDrag({ start: p, current: p })
          onChange(undefined)
        }}
        onPointerMove={(e) => {
          if (!drag) return
          const p = toNormalizedPoint(e.clientX, e.clientY)
          if (!p) return
          const next = { start: drag.start, current: p }
          setDrag(next)
        }}
        onPointerUp={(e) => {
          if (!drag) return
          try {
            const rect = normalizeRect(drag.start, drag.current)
            if (rect.width > 0.005 && rect.height > 0.005) onChange(rect)
          } finally {
            setDrag(null)
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
