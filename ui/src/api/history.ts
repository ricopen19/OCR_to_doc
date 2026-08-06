import { invoke } from '@tauri-apps/api/core'

export type RecentResultEntry = {
  dirName: string
  updatedAtMs: number
  pageRange?: string
  bestFile?: string
}

export type EnvironmentStatus = {
  projectRoot: string
  os: string
  resultDirFound: boolean
  resultRoot: string
  pythonBin: string
  pythonFound: boolean
  pythonPath?: string
  popplerFound: boolean
  popplerPath?: string
  resourceRoots?: string[]
  ollamaRunning: boolean
  ocrModelReady: boolean
  ocrModelName: string
}

export async function listRecentResults(limit = 10): Promise<RecentResultEntry[]> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (hasTauri) return invoke<RecentResultEntry[]>('list_recent_results', { limit })
  return []
}

export async function openResultDir(dirName: string): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (hasTauri) return invoke('open_result_dir', { dirName })
  console.log('Mock open result dir:', dirName)
}

export async function openResultFile(dirName: string): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (hasTauri) return invoke('open_result_file', { dirName })
  console.log('Mock open result file:', dirName)
}

export async function checkEnvironment(): Promise<EnvironmentStatus> {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (hasTauri) return invoke<EnvironmentStatus>('check_environment')
  return {
    projectRoot: '',
    os: '',
    resultDirFound: false,
    resultRoot: '',
    pythonBin: '',
    pythonFound: false,
    pythonPath: undefined,
    popplerFound: false,
    popplerPath: undefined,
    resourceRoots: [],
    ollamaRunning: false,
    ocrModelReady: false,
    ocrModelName: 'glm-ocr',
  }
}
