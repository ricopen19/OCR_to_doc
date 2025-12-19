import { useEffect, useRef, useState } from 'react'
import {
  AppShell,
  Burger,
  Group,
  Title,
  Anchor,
  Container,
} from '@mantine/core'
import { runJob, getProgress, getResult } from './api/runJob'
import { Sidebar, type PageKey } from './components/Layout/Sidebar'
import { Home } from './pages/Home'
import { RunJob } from './pages/RunJob'
import { Result } from './pages/Result'
import { Settings } from './pages/Settings'
import { loadSettings } from './api/settings'

function App() {
  const [opened, setOpened] = useState(false)
  const [page, setPage] = useState<PageKey>('home')

  // Global State for Job
  const [filePaths, setFilePaths] = useState<string[]>([])
  const [status, setStatus] = useState<'idle' | 'running' | 'done'>('idle')
  const [resultText, setResultText] = useState<string>('')
  const [error, setError] = useState<string | null>(null)
  const [log, setLog] = useState<string[]>([])
  const logCursorRef = useRef(0)
  const [jobId, setJobId] = useState<string | null>(null)
  const [progress, setProgress] = useState<number>(0)
  const [outputs, setOutputs] = useState<string[]>([])
  const [options, setOptions] = useState({
    formats: ['md', 'docx', 'xlsx'],
    imageAsPdf: false,
    enableFigure: true,
  })

  // Polling logic
  useEffect(() => {
    // initialize options from settings
    loadSettings().then((s) => {
      setOptions({
        formats: s.formats,
        imageAsPdf: s.imageAsPdf,
        enableFigure: s.enableFigure,
      })
    }).catch(console.error)
  }, [])

  useEffect(() => {
    if (!jobId || status !== 'running') return
    const timer = setInterval(async () => {
      try {
        const p = await getProgress(jobId)
        setProgress(p.progress ?? 0)
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
    setStatus('running')
    setResultText('')
    setError(null)
    setLog([])
    logCursorRef.current = 0
    setProgress(0)
    try {
      const job = await runJob(filePaths, options)
      setJobId(job.jobId)
    } catch (err) {
      setError(String(err))
      setStatus('idle')
    }
  }

  return (
    <AppShell
      header={{ height: 60 }}
      navbar={{ width: 260, breakpoint: 'sm', collapsed: { mobile: !opened } }}
      padding="lg"
      styles={{
        main: { backgroundColor: '#f8fafc' },
      }}
    >
      <AppShell.Header>
        <Group h="100%" px="md" justify="space-between">
          <Group gap="sm">
            <Burger opened={opened} onClick={() => setOpened((o) => !o)} hiddenFrom="sm" size="sm" />
            <Title order={4} style={{ letterSpacing: '-0.5px' }}>OCR to Doc</Title>
          </Group>
          <Anchor size="sm" c="dimmed" href="#" target="_blank" rel="noreferrer">
            v0.1.0
          </Anchor>
        </Group>
      </AppShell.Header>

      <AppShell.Navbar p="md" withBorder={false} style={{ backgroundColor: 'transparent' }}>
        <Sidebar activePage={page} setPage={setPage} />
      </AppShell.Navbar>

      <AppShell.Main>
        <Container size="lg" px="md">
          {page === 'home' && <Home onNavigate={setPage} />}
          {page === 'run' && (
            <RunJob
              filePaths={filePaths}
              setFilePaths={setFilePaths}
              status={status}
              setStatus={setStatus}
              progress={progress}
              log={log}
              error={error}
              setError={setError}
              onRun={handleRun}
              options={options}
              setOptions={setOptions}
            />
          )}
          {page === 'result' && (
            <Result
              outputs={outputs}
              resultText={resultText}
              error={error}
              jobId={jobId}
            />
          )}
          {page === 'settings' && (
            <Settings />
          )}
        </Container>
      </AppShell.Main>
    </AppShell>
  )
}

export default App
