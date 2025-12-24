import { invoke } from '@tauri-apps/api/core'

export type RecentResultEntry = {
  dirName: string
  updatedAtMs: number
  pageRange?: string
  bestFile?: string
}

export type EnvironmentStatus = {
  projectRoot: string
  dispatcherFound: boolean
  resultDirFound: boolean
  pythonBin: string
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
    dispatcherFound: false,
    resultDirFound: false,
    pythonBin: '',
  }
}

