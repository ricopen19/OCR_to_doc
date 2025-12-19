import { invoke } from '@tauri-apps/api/core'

export type RunOptions = {
  formats: string[]
  imageAsPdf: boolean
  enableFigure: boolean
}

export type ProgressPayload = {
  status: 'idle' | 'running' | 'done' | 'error'
  progress?: number
  log?: string[]
  error?: string
}

export type ResultPayload = {
  outputs?: string[]
  preview?: string
}

export async function runJob(paths: string[], options: RunOptions) {
  const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
  if (!paths.length) throw new Error('no input files')
  if (hasTauri) {
    return invoke<{ jobId: string }>('run_job', { paths, options })
  }
  // dev server モック
  await new Promise((r) => setTimeout(r, 120))
  return { jobId: 'mock-job' }
}

export async function getProgress(jobId: string): Promise<ProgressPayload> {
  const hasTauri = typeof window !== 'undefined' && '__TAURI__' in window
  if (hasTauri) return invoke<ProgressPayload>('get_progress', { jobId })
  return { status: 'done', progress: 100 }
}

export async function getResult(jobId: string): Promise<ResultPayload> {
  const hasTauri = typeof window !== 'undefined' && '__TAURI__' in window
  if (hasTauri) return invoke<ResultPayload>('get_result', { jobId })
  return {
    outputs: ['sample.md', 'sample.docx', 'sample.xlsx'],
    preview: 'Converted markdown for: sample.pdf',
  }
}

export async function saveFile(jobId: string, filename: string, destPath: string): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && '__TAURI__' in window
  if (hasTauri) {
    return invoke('save_file', { jobId, filename, destPath })
  }
  // dev server mock
  console.log('Mock save:', filename, 'to', destPath)
  await new Promise((r) => setTimeout(r, 500))
}
