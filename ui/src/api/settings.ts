
import { invoke } from '@tauri-apps/api/core'

export interface AppSettings {
    formats: string[]
    imageAsPdf: boolean
    enableFigure: boolean
    outputRoot?: string
}

const DEFAULT_SETTINGS: AppSettings = {
    formats: ['md'],
    imageAsPdf: false,
    enableFigure: true,
}

export async function loadSettings(): Promise<AppSettings> {
    const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
    if (!hasTauri) {
        // dev mock
        return Promise.resolve(DEFAULT_SETTINGS)
    }
    return invoke<AppSettings>('load_settings')
}

export async function saveSettings(settings: AppSettings): Promise<void> {
    const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
    if (!hasTauri) {
        console.log('[Mock] Saving Settings:', settings)
        return Promise.resolve()
    }
    return invoke('save_settings', { settings })
}
