import { useEffect, useRef, useState } from 'react'
import {
  AppShell,
  Burger,
  Group,
  Title,
  Anchor,
  Container,
  Modal,
  Text,
  Button,
  Stack,
} from '@mantine/core'
import { runJob, getProgress, getResult } from './api/runJob'
import { Sidebar, type PageKey } from './components/Layout/Sidebar'
import { Home } from './pages/Home'
import { RunJob } from './pages/RunJob'
import { Result } from './pages/Result'
import { Settings, type SettingsHandle } from './pages/Settings'
import { HelpAbout } from './pages/HelpAbout'
import { HelpHowToUse } from './pages/HelpHowToUse'
import { HelpTroubleshooting } from './pages/HelpTroubleshooting'
import { loadSettings, type AppSettings } from './api/settings'
import type { CropRect } from './types/crop'

function App() {
  const [opened, setOpened] = useState(true)
  const [page, setPage] = useState<PageKey>('home')
  const [viewportWidth, setViewportWidth] = useState<number>(() =>
    typeof window === 'undefined' ? 0 : window.innerWidth
  )

  const NAVBAR_WIDTH = 200
  const COLLAPSE_RATIO = 0.35
  const shouldAutoCollapse = viewportWidth > 0 ? NAVBAR_WIDTH / viewportWidth > COLLAPSE_RATIO : false

  // Global State for Job
  const [filePaths, setFilePaths] = useState<string[]>([])
  const [status, setStatus] = useState<'idle' | 'running' | 'done'>('idle')
  const [resultText, setResultText] = useState<string>('')
  const [error, setError] = useState<string | null>(null)
  const [log, setLog] = useState<string[]>([])
  const logCursorRef = useRef(0)
  const [jobId, setJobId] = useState<string | null>(null)
  const [progress, setProgress] = useState<number>(0)
  const [currentMessage, setCurrentMessage] = useState<string>('')
  const [etaSeconds, setEtaSeconds] = useState<number | null>(null)
  const [outputs, setOutputs] = useState<string[]>([])
  const [jobInputPaths, setJobInputPaths] = useState<string[]>([])
  const [tempInputPaths, setTempInputPaths] = useState<string[]>([])
  const jobCleanupPathsRef = useRef<string[]>([])
  const settingsRef = useRef<SettingsHandle | null>(null)
  const [navConfirmOpen, setNavConfirmOpen] = useState(false)
  const [navPendingPage, setNavPendingPage] = useState<PageKey | null>(null)
  const [navSaving, setNavSaving] = useState(false)
  const [previewQuality, setPreviewQuality] = useState<'light' | 'standard' | 'high'>('light')
  const [options, setOptions] = useState<{
    formats: string[]
    imageAsPdf: boolean
    enableFigure: boolean
    useGpu: boolean
    mode: 'lite' | 'full'
    docxEngine: 'python' | 'pandoc'
    excelMode: 'layout' | 'table'
    excelMetaSheet: boolean
    excelSymbolFallback: boolean
    chunkSize: number
    enableRest: boolean
    restSeconds: number
    pdfDpi: number
    fileOptions: Record<string, { start?: number; end?: number; crop?: CropRect }>
  }>({
    formats: ['md'],
    imageAsPdf: false,
    enableFigure: false,
    useGpu: false,
    mode: 'lite',
    docxEngine: 'python',
    excelMode: 'layout',
    excelMetaSheet: true,
    excelSymbolFallback: true,
    chunkSize: 10,
    enableRest: true,
    restSeconds: 5,
    pdfDpi: 150,
    fileOptions: {},
  })

  const closeNavConfirm = () => {
    setNavConfirmOpen(false)
    setNavPendingPage(null)
    setNavSaving(false)
  }

  const navigate = (nextPage: PageKey) => {
    if (nextPage === page) return
    if (page === 'settings' && settingsRef.current?.isDirty()) {
      setNavPendingPage(nextPage)
      setNavConfirmOpen(true)
      return
    }
    setPage(nextPage)
  }

  // Polling logic
  useEffect(() => {
    // initialize options from settings
    loadSettings().then((s) => {
        setOptions((prev) => ({
          ...prev,
          formats: s.formats,
          imageAsPdf: s.imageAsPdf,
          enableFigure: s.enableFigure,
          useGpu: Boolean(s.useGpu),
          mode: s.mode ?? 'lite',
          docxEngine: s.docxEngine ?? 'python',
          excelMode: s.excelMode ?? 'layout',
          excelMetaSheet: s.excelMetaSheet ?? true,
          excelSymbolFallback: s.excelSymbolFallback ?? true,
          chunkSize: s.chunkSize ?? 10,
          enableRest: s.enableRest,
          restSeconds: s.restSeconds ?? 10,
          pdfDpi: s.pdfDpi ?? 150,
        }))
        setPreviewQuality(s.previewQuality ?? 'light')
    }).catch(console.error)
  }, [])

  useEffect(() => {
    const onResize = () => setViewportWidth(window.innerWidth)
    onResize()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  useEffect(() => {
    if (shouldAutoCollapse) setOpened(false)
  }, [shouldAutoCollapse])

  const handleSettingsSaved = (s: AppSettings) => {
    setOptions((prev) => ({
      ...prev,
      formats: s.formats,
      imageAsPdf: s.imageAsPdf,
      enableFigure: s.enableFigure,
      useGpu: Boolean(s.useGpu),
      mode: s.mode ?? 'lite',
      excelMode: s.excelMode ?? 'layout',
      excelMetaSheet: s.excelMetaSheet ?? true,
      excelSymbolFallback: s.excelSymbolFallback ?? true,
      chunkSize: s.chunkSize ?? 10,
      enableRest: s.enableRest,
      restSeconds: s.restSeconds ?? 10,
      pdfDpi: s.pdfDpi ?? 150,
    }))
    setPreviewQuality(s.previewQuality ?? 'light')
  }

  useEffect(() => {
    if (!jobId || status !== 'running') return
    const timer = setInterval(async () => {
      try {
        const p = await getProgress(jobId)
        setProgress(p.progress ?? 0)
        setCurrentMessage(p.currentMessage ?? '')
        setEtaSeconds(typeof p.etaSeconds === 'number' ? p.etaSeconds : null)
        if (p.log) {
          const prevLen = logCursorRef.current
          const nextLen = p.log.length
          const newLines = nextLen >= prevLen ? p.log.slice(prevLen) : p.log
          logCursorRef.current = nextLen
          if (newLines.length) setLog((prev) => [...prev, ...newLines].slice(-200))
        }
        if (p.status === 'done') {
          setStatus('done')
          clearInterval(timer)
          const res = await getResult(jobId)
          setResultText(res.preview ?? '')
          setOutputs(res.outputs ?? [])
          setPage('result')
          const cleanupPaths = jobCleanupPathsRef.current
          if (cleanupPaths.length > 0) {
            setFilePaths((prev) => prev.filter((p) => !cleanupPaths.includes(p)))
            setTempInputPaths((prev) => prev.filter((p) => !cleanupPaths.includes(p)))
            setOptions((prev) => {
              if (!prev.fileOptions || Object.keys(prev.fileOptions).length === 0) return prev
              const nextFileOptions = { ...prev.fileOptions }
              let changed = false
              for (const path of cleanupPaths) {
                if (path in nextFileOptions) {
                  delete nextFileOptions[path]
                  changed = true
                }
              }
              if (!changed) return prev
              return { ...prev, fileOptions: nextFileOptions }
            })
            jobCleanupPathsRef.current = []
          }
        } else if (p.status === 'error') {
          setError(p.error ?? 'unknown error')
          setStatus('idle')
          clearInterval(timer)
        }
      } catch (e) {
        setError(String(e))
        setStatus('idle')
        clearInterval(timer)
      }
    }, 800)
    return () => clearInterval(timer)
  }, [jobId, status])

  const handleRun = async () => {
    if (filePaths.length === 0) return
    setJobInputPaths(filePaths.slice())
    setStatus('running')
    setResultText('')
    setError(null)
    setLog([])
    logCursorRef.current = 0
    setProgress(0)
    setCurrentMessage('')
    setEtaSeconds(null)
    try {
      const cleanupPaths = tempInputPaths.filter((p) => filePaths.includes(p))
      jobCleanupPathsRef.current = cleanupPaths
      const job = await runJob(filePaths, options, cleanupPaths)
      setJobId(job.jobId)
    } catch (err) {
      setError(String(err))
      setStatus('idle')
    }
  }

  return (
    <>
      <Modal
        opened={navConfirmOpen}
        onClose={() => {
          if (navSaving) return
          closeNavConfirm()
        }}
        centered
        withCloseButton={!navSaving}
        closeOnClickOutside={!navSaving}
        closeOnEscape={!navSaving}
        title="移動前に確認"
      >
        <Stack gap="md">
          <Text>変更内容が保存されていません。移動前に保存しますか？</Text>
          <Group justify="flex-end" gap="sm">
            <Button
              variant="default"
              disabled={navSaving}
              onClick={() => {
                if (!navPendingPage) return closeNavConfirm()
                const next = navPendingPage
                closeNavConfirm()
                setPage(next)
              }}
            >
              保存せず移動
            </Button>
            <Button
              variant="subtle"
              disabled={navSaving}
              onClick={() => closeNavConfirm()}
            >
              キャンセル
            </Button>
            <Button
              loading={navSaving}
              onClick={async () => {
                if (!navPendingPage) return closeNavConfirm()
                const handle = settingsRef.current
                if (!handle) {
                  const next = navPendingPage
                  closeNavConfirm()
                  setPage(next)
                  return
                }
                setNavSaving(true)
                const ok = await handle.save()
                if (!ok) {
                  setNavSaving(false)
                  return
                }
                const next = navPendingPage
                closeNavConfirm()
                setPage(next)
              }}
            >
              保存して移動
            </Button>
          </Group>
        </Stack>
      </Modal>

      <AppShell
        header={{ height: 60 }}
        navbar={{
          width: NAVBAR_WIDTH,
          // breakpoint を指定すると狭い幅で navbar が 100% 幅になるため、比率ベースの制御に寄せる
          breakpoint: 0,
          collapsed: {
            desktop: !opened,
          },
        }}
        padding="md"
        styles={{
          main: { backgroundColor: '#f8fafc' },
        }}
      >
        <AppShell.Header>
          <Group h="100%" px="md" justify="space-between">
            <Group gap="sm">
              <Burger opened={opened} onClick={() => setOpened((o) => !o)} size="sm" />
              <Title order={4} style={{ letterSpacing: '-0.5px' }}>OCR to Doc</Title>
            </Group>
            <Anchor size="sm" c="dimmed" href="#" target="_blank" rel="noreferrer">
              v0.1.0
            </Anchor>
          </Group>
        </AppShell.Header>

        <AppShell.Navbar p="md" withBorder={false}>
          <Sidebar activePage={page} setPage={navigate} />
        </AppShell.Navbar>

        <AppShell.Main>
          <Container size="lg" px="md">
            {page === 'home' && <Home onNavigate={(p) => navigate(p)} />}
            {page === 'run' && (
              <RunJob
                filePaths={filePaths}
                setFilePaths={setFilePaths}
                addTempPaths={(paths) =>
                  setTempInputPaths((prev) => Array.from(new Set([...prev, ...paths])))
                }
                clearTempPaths={() => setTempInputPaths([])}
                status={status}
                setStatus={setStatus}
                progress={progress}
                currentMessage={currentMessage}
                etaSeconds={etaSeconds}
                log={log}
                error={error}
                setError={setError}
                onRun={handleRun}
                options={options}
                setOptions={setOptions}
                previewQuality={previewQuality}
              />
            )}
            {page === 'result' && (
              <Result
                outputs={outputs}
                resultText={resultText}
                error={error}
                jobId={jobId}
                inputPaths={jobInputPaths}
              />
            )}
            {page === 'settings' && (
              <Settings ref={settingsRef} onSaved={handleSettingsSaved} />
            )}
            {page === 'help_howto' && <HelpHowToUse />}
            {page === 'help_troubleshooting' && <HelpTroubleshooting />}
            {page === 'help_about' && <HelpAbout />}
          </Container>
        </AppShell.Main>
      </AppShell>
    </>
  )
}

export default App
