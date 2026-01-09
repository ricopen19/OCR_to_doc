import { invoke } from '@tauri-apps/api/core'

export async function openReadme(): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (hasTauri) {
    await invoke('open_readme')
    return
  }
  console.log('Mock open README')
}
