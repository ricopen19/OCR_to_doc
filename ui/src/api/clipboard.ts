import { invoke } from '@tauri-apps/api/core'

export async function saveClipboardImage(data: Uint8Array, extension: string): Promise<string> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (!hasTauri) {
    throw new Error('clipboard image save requires Tauri runtime')
  }
  return invoke<string>('save_clipboard_image', {
    data: Array.from(data),
    extension,
  })
}
