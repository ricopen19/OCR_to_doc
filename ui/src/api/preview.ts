import { invoke } from '@tauri-apps/api/core'
import type { CropRect } from '../types/crop'

export type PreviewPayload = {
  dataUrl: string
  pageCount?: number | null
  page?: number | null
}

export async function renderPreview(
  path: string,
  opts?: { page?: number; crop?: CropRect; maxLongEdge?: number },
): Promise<PreviewPayload> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (!hasTauri) throw new Error('Tauri 環境外ではプレビューできません')
  return invoke<PreviewPayload>('render_preview', {
    path,
    page: opts?.page,
    crop: opts?.crop,
    maxLongEdge: opts?.maxLongEdge,
  })
}
