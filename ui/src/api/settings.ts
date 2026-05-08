
import { invoke } from '@tauri-apps/api/core'

export interface AppSettings {
    formats: string[]
    imageAsPdf: boolean
    enableFigure: boolean
    mode?: 'lite' | 'full'
    docxEngine?: 'python' | 'pandoc'
    excelMode?: 'layout' | 'table'
    outputRoot?: string
    excelMetaSheet?: boolean
    excelSymbolFallback?: boolean
    chunkSize?: number
    enableRest: boolean
    restSeconds?: number
    pdfDpi?: number
    windowWidth?: number
    windowHeight?: number
    useGpu?: boolean
    previewQuality?: 'light' | 'standard' | 'high'
}

const DEFAULT_SETTINGS: AppSettings = {
    formats: ['md'],
    imageAsPdf: false,
    enableFigure: false,
    mode: 'lite',
    docxEngine: 'python',
    excelMode: 'layout',
    excelMetaSheet: true,
    excelSymbolFallback: true,
    chunkSize: 10,
    enableRest: true,
    restSeconds: 5,
    pdfDpi: 150,
    windowWidth: 1200,
    windowHeight: 760,
    useGpu: false,
    previewQuality: 'light',
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

export async function getCurrentWindowSize(): Promise<{ width: number; height: number }> {
    const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
    if (hasTauri) {
        const { getCurrentWindow } = await import('@tauri-apps/api/window')
        const win = getCurrentWindow()
        const [size, factor] = await Promise.all([win.innerSize(), win.scaleFactor()])
        const width = Math.max(0, Math.round(size.width / factor))
        const height = Math.max(0, Math.round(size.height / factor))
        return { width, height }
    }
    return { width: window.innerWidth, height: window.innerHeight }
}
