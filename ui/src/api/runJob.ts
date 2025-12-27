import { invoke } from '@tauri-apps/api/core'
import type { CropRect } from '../types/crop'

export type FileSpecificOptions = {
  start?: number
  end?: number
  crop?: CropRect
}

export type RunOptions = {
  formats: string[]
  imageAsPdf: boolean
  enableFigure: boolean
  useGpu?: boolean
  mode: 'lite' | 'full'
  excelMode?: 'layout' | 'table'
  excelMetaSheet?: boolean
  chunkSize?: number
  enableRest: boolean
  restSeconds?: number
  pdfDpi?: number
  fileOptions?: Record<string, FileSpecificOptions>
}

export type ProgressPayload = {
  status: 'idle' | 'running' | 'done' | 'error'
  progress?: number
  log?: string[]
  error?: string
  currentMessage?: string
  pageCurrent?: number
  pageTotal?: number
  etaSeconds?: number
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

export async function openOutput(jobId: string, filename: string): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && '__TAURI__' in window
  if (hasTauri) {
    return invoke('open_output', { jobId, filename })
  }
  console.log('Mock open:', filename)
}

export async function openOutputDir(jobId: string): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && '__TAURI__' in window
  if (hasTauri) {
    return invoke('open_output_dir', { jobId })
  }
  console.log('Mock open dir for job:', jobId)
}

export async function openInputFile(path: string): Promise<void> {
  const hasTauri = typeof window !== 'undefined' && '__TAURI__' in window
  if (hasTauri) {
    return invoke('open_input_file', { path })
  }
  console.log('Mock open input file:', path)
}
