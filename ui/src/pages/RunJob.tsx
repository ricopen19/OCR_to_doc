import {
    Stack,
    Title,
    Text,
    Card,
    SimpleGrid,
    Box,
    Button,
    ThemeIcon,
    CheckboxGroup,
    Checkbox,
    Divider,
    Group,
    Badge,
    Progress,
    Alert,
    Container,
    SegmentedControl,
    NumberInput,
    Radio,
    Switch,
} from '@mantine/core'
import { IconUpload, IconPlayerPlay, IconFile, IconX, IconAlertTriangle, IconCrop, IconDeviceFloppy } from '@tabler/icons-react'
import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import { getPdfPageCount, detectPdfText, type PdfTextDetection } from '../api/runJob'
import type { CropRect } from '../types/crop'
import { CropModal } from '../components/CropModal'
import { open } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'
import { notifications } from '@mantine/notifications'
import { loadSettings, saveSettings, type AppSettings } from '../api/settings'
import { saveClipboardImage } from '../api/clipboard'

type FileWithPath = File & { path?: string }

function hasTauriRuntime() {
    return typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
}

function resolveImageExtension(mime: string) {
    const lowered = mime.toLowerCase()
    if (lowered.includes('png')) return 'png'
    if (lowered.includes('jpeg') || lowered.includes('jpg')) return 'jpg'
    if (lowered.includes('bmp')) return 'bmp'
    if (lowered.includes('gif')) return 'gif'
    if (lowered.includes('webp')) return 'webp'
    if (lowered.includes('tiff') || lowered.includes('tif')) return 'tiff'
    return 'png'
}

function findClipboardImages(items: DataTransferItemList | undefined | null) {
    if (!items) return []
    const files: File[] = []
    for (const item of Array.from(items)) {
        if (item.kind === 'file' && item.type.startsWith('image/')) {
            const file = item.getAsFile()
            if (file) files.push(file)
        }
    }
    return files
}

export interface RunJobOptions {
    formats: string[]
    enableFigure: boolean
    docxEngine: 'python' | 'pandoc'
    excelMode: 'layout' | 'table'
    excelMetaSheet: boolean
    enableRest: boolean
    restSeconds: number
    pdfDpi: number
    fileOptions: Record<string, { start?: number; end?: number; crop?: CropRect }>
    useEmbeddedText: boolean
    ocrEngine: 'ollama' | 'llamacpp'
    ocrModel: string
    llamaBaseUrl?: string
    llamaApiKey?: string
    llamaModel?: string
}

interface RunJobProps {
    filePaths: string[]
    setFilePaths: (paths: string[]) => void
    addTempPaths: (paths: string[]) => void
    clearTempPaths: () => void
    status: 'idle' | 'running' | 'done'
    setStatus: (status: 'idle' | 'running' | 'done') => void
    progress: number
    currentMessage: string
    etaSeconds: number | null
    log: string[]
    error: string | null
    setError: (error: string | null) => void
    onRun: () => void
    options: RunJobOptions
    setOptions: Dispatch<SetStateAction<RunJobOptions>>
    previewQuality: 'light' | 'standard' | 'high'
}

export function RunJob({
    filePaths,
    setFilePaths,
    addTempPaths,
    clearTempPaths,
    status,
    progress,
    currentMessage,
    etaSeconds,
    log,
    error,
    setError,
    onRun,
    options,
    setOptions,
    previewQuality,
}: RunJobProps) {
    const fileInputRef = useRef<HTMLInputElement | null>(null)
    const logBoxRef = useRef<HTMLDivElement | null>(null)
    const filePathsRef = useRef<string[]>([])
    const [cropTarget, setCropTarget] = useState<string | null>(null)
    const [savingDefaults, setSavingDefaults] = useState(false)
    const deriveDpiPreset = (dpi: number) =>
        ([100, 150, 200, 300].includes(dpi) ? String(dpi) : 'custom') as '100' | '150' | '200' | '300' | 'custom'

    const [dpiPreset, setDpiPreset] = useState<'100' | '150' | '200' | '300' | 'custom'>(() =>
        deriveDpiPreset(options.pdfDpi ?? 150)
    )
    const [pageCountMap, setPageCountMap] = useState<Record<string, number>>({})
    const [pdfTextInfoMap, setPdfTextInfoMap] = useState<Record<string, PdfTextDetection>>({})

    useEffect(() => {
        setDpiPreset(deriveDpiPreset(options.pdfDpi ?? 150))
    }, [options.pdfDpi])

    useEffect(() => {
        const pdfs = filePaths.filter((p) => p.toLowerCase().endsWith('.pdf'))
        for (const p of pdfs) {
            if (p in pageCountMap) continue
            void getPdfPageCount(p).then((count) => {
                if (count != null) setPageCountMap((prev) => ({ ...prev, [p]: count }))
            })
        }
    }, [filePaths]) // eslint-disable-line react-hooks/exhaustive-deps

    useEffect(() => {
        const pdfs = filePaths.filter((p) => p.toLowerCase().endsWith('.pdf'))
        for (const p of pdfs) {
            if (p in pdfTextInfoMap) continue
            void detectPdfText(p).then((info) => {
                if (info != null) setPdfTextInfoMap((prev) => ({ ...prev, [p]: info }))
            })
        }
    }, [filePaths]) // eslint-disable-line react-hooks/exhaustive-deps

    // 選択中の PDF のいずれかが埋め込みテキストを持つ（TextBased / Mixed）場合のみ、
    // 「埋め込みテキストを使用する」オプションを提示する。
    const hasEligiblePdfText = Object.values(pdfTextInfoMap).some((info) => info.eligible)

    useEffect(() => {
        const el = logBoxRef.current
        if (!el) return
        requestAnimationFrame(() => {
            el.scrollTop = el.scrollHeight
        })
    }, [log.length])

    useEffect(() => {
        filePathsRef.current = filePaths
    }, [filePaths])

    const handleClipboardImages = useCallback(
        async (files: File[]) => {
            if (files.length === 0) return
            if (!hasTauriRuntime()) {
                notifications.show({
                    title: 'クリップボード',
                    message: 'クリップボード貼り付けはデスクトップ版で利用できます',
                    color: 'yellow',
                })
                return
            }
            try {
                const savedPaths: string[] = []
                for (const file of files) {
                    const buffer = await file.arrayBuffer()
                    const ext = resolveImageExtension(file.type || 'image/png')
                    const path = await saveClipboardImage(new Uint8Array(buffer), ext)
                    savedPaths.push(path)
                }
                if (savedPaths.length > 0) {
                    const merged = Array.from(new Set([...filePathsRef.current, ...savedPaths]))
                    setFilePaths(merged)
                    addTempPaths(savedPaths)
                }
                setError(null)
            } catch (err) {
                console.error(err)
                notifications.show({
                    title: '貼り付けに失敗しました',
                    message: String(err),
                    color: 'red',
                })
            }
        },
        [addTempPaths, setError, setFilePaths]
    )

    useEffect(() => {
        if (!hasTauriRuntime()) return
        const onPaste = (event: ClipboardEvent) => {
            if (status === 'running') return
            const target = event.target as HTMLElement | null
            if (target && (target.isContentEditable || target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) {
                return
            }
            const files = findClipboardImages(event.clipboardData?.items)
            if (files.length === 0) return
            event.preventDefault()
            void handleClipboardImages(files)
        }
        window.addEventListener('paste', onPaste)
        return () => window.removeEventListener('paste', onPaste)
    }, [handleClipboardImages, status])

    useEffect(() => {
        if (!hasTauriRuntime()) return
        let unlisten: (() => void) | null = null
        void (async () => {
            const handler = (event: { payload: unknown }) => {
                const payload = event.payload
                let dropped: string[] = []
                if (Array.isArray(payload)) {
                    dropped = payload.filter((p): p is string => typeof p === 'string')
                } else if (payload && typeof payload === 'object' && 'paths' in payload) {
                    const maybe = (payload as { paths?: unknown }).paths
                    if (Array.isArray(maybe)) dropped = maybe.filter((p): p is string => typeof p === 'string')
                } else if (typeof payload === 'string') {
                    dropped = [payload]
                }
                if (dropped.length > 0) {
                    const merged = Array.from(new Set([...filePathsRef.current, ...dropped]))
                    setFilePaths(merged)
                    setError(null)
                }
            }

            // Tauri v2 drag & drop events
            unlisten = await listen<unknown>('tauri://drag-drop', handler as any)
            // Backward compatibility / older event name (no-op if never emitted)
            const unlistenLegacy = await listen<unknown>('tauri://file-drop', handler as any)
            const prev = unlisten
            unlisten = () => {
                prev()
                unlistenLegacy()
            }
        })()
        return () => {
            if (unlisten) unlisten()
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [])

    const handleSaveDefaults = async () => {
        if (savingDefaults) return
        setSavingDefaults(true)
        try {
            const current = await loadSettings()
            const toSave: AppSettings = {
                ...current,
                formats: options.formats,
                excelMode: options.excelMode,
                pdfDpi: options.pdfDpi,
                docxEngine: options.docxEngine,
            }
            await saveSettings(toSave)
            notifications.show({
                title: '保存しました',
                message: '実行画面の設定をデフォルトとして保存しました',
                color: 'green',
            })
        } catch (err) {
            console.error(err)
            notifications.show({
                title: 'エラー',
                message: 'デフォルト設定の保存に失敗しました: ' + String(err),
                color: 'red',
            })
        } finally {
            setSavingDefaults(false)
        }
    }

    const chooseFiles = async () => {
        if (!hasTauriRuntime()) return
        const selected = await open({
            multiple: true,
            filters: [
                { name: 'Input', extensions: ['pdf', 'heic', 'heif', 'jpg', 'jpeg', 'png'] },
            ],
        })
        if (!selected) return
        const paths = Array.isArray(selected) ? selected : [selected]
        if (paths.length > 0) {
            const merged = Array.from(new Set([...filePathsRef.current, ...paths]))
            setFilePaths(merged)
            setError(null)
        }
    }

    const formatEta = (secs: number) => {
        const s = Math.max(0, Math.floor(secs))
        const m = Math.floor(s / 60)
        const r = s % 60
        if (m <= 0) return `${r}秒`
        return `${m}分${r.toString().padStart(2, '0')}秒`
    }

    return (
        <Container size="lg" px={0}>
            <Stack gap="xl">
                <Stack gap={4}>
                    <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                        OCR 実行
                    </Title>
                    <Text c="dimmed">ファイルをアップロードして設定を選択してください。</Text>
                </Stack>

                <SimpleGrid cols={1} spacing="lg">
                    {/* Left Column: Input */}
                    <Stack gap="lg">
                        <Card withBorder shadow="sm" radius="lg" padding="lg">
                            <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                                入力ファイル
                            </Text>

                            <Box
                                onClick={() => {
                                    if (hasTauriRuntime()) {
                                        void chooseFiles()
                                    } else {
                                        fileInputRef.current?.click()
                                    }
                                }}
                                onDragOver={(e) => {
                                    e.preventDefault()
                                    e.currentTarget.style.borderColor = 'var(--mantine-color-blue-5)'
                                    e.currentTarget.style.backgroundColor = 'var(--mantine-color-blue-0)'
                                }}
                                onDragLeave={(e) => {
                                    e.preventDefault()
                                    e.currentTarget.style.borderColor = 'var(--mantine-color-gray-3)'
                                    e.currentTarget.style.backgroundColor = 'var(--mantine-color-gray-0)'
                                }}
                                onDrop={(e) => {
                                    e.preventDefault()
                                    e.currentTarget.style.borderColor = 'var(--mantine-color-gray-3)'
                                    e.currentTarget.style.backgroundColor = 'var(--mantine-color-gray-0)'
                                    // In Tauri, file system paths are provided via `tauri://drag-drop` event.
                                    if (hasTauriRuntime()) return
                                    const files = Array.from(e.dataTransfer?.files || [])
                                    const paths = files.map((f) => {
                                        const file = f as FileWithPath
                                        return file.path ?? file.name
                                    })
                                    if (paths.length > 0) {
                                        setFilePaths(paths)
                                        setError(null)
                                    }
                                }}
                                style={{
                                    border: '2px dashed var(--mantine-color-gray-3)',
                                    borderRadius: 'var(--mantine-radius-lg)',
                                    backgroundColor: 'var(--mantine-color-gray-0)',
                                    minHeight: 180,
                                    display: 'flex',
                                    flexDirection: 'column',
                                    alignItems: 'center',
                                    justifyContent: 'center',
                                    cursor: 'pointer',
                                    transition: 'all 0.2s ease',
                                }}
                            >
                                <Stack gap="xs" align="center" style={{ pointerEvents: 'none' }}>
                                    <ThemeIcon size={48} radius="xl" variant="light" color="blue">
                                        <IconUpload size={24} />
                                    </ThemeIcon>
                                    <Text fw={600} size="md">クリック / ドラッグ＆ドロップ / 貼り付け (Ctrl/Cmd+V)</Text>
                                    <Text size="sm" c="dimmed">
                                        PDF, HEIC, JPG, PNG
                                    </Text>
                                </Stack>
                                {!hasTauriRuntime() && (
                                    <input
                                        ref={fileInputRef}
                                        type="file"
                                        multiple
                                        accept=".pdf,.heic,.heif,.jpg,.jpeg,.png"
                                        style={{ display: 'none' }}
                                        onChange={(e) => {
                                            const files = Array.from(e.target.files || [])
                                            const paths = files.map((f) => {
                                                const file = f as FileWithPath
                                                return file.path ?? file.name
                                            })
                                            if (paths.length > 0) {
                                                setFilePaths(paths)
                                                setError(null)
                                            }
                                        }}
                                    />
                                )}
                            </Box>

                            {filePaths.length > 0 && (
                                <Stack gap="xs" mt="lg">
                                    <Group justify="space-between">
                                        <Text size="sm" fw={600}>選択されたファイル ({filePaths.length})</Text>
                                        <Button
                                            variant="subtle"
                                            color="red"
                                            size="compact-xs"
                                            onClick={(e) => {
                                                e.stopPropagation();
                                                setFilePaths([]);
                                                clearTempPaths();
                                                setOptions(prev => ({ ...prev, fileOptions: {} }));
                                            }}
                                        >
                                            クリア
                                        </Button>
                                    </Group>
                                    <Stack gap="xs">
                                        {filePaths.map((p) => {
                                            const isPdf = p.toLowerCase().endsWith('.pdf');
                                            const opts = options.fileOptions[p] || {};
                                            const pageCount = pageCountMap[p]
                                            const isHeavy = pageCount != null && pageCount > 50
                                            return (
                                                <Card key={p} withBorder radius="md" padding="sm" bg={isHeavy ? 'yellow.0' : 'gray.0'}>
                                                    <Stack gap="xs">
                                                        <Group justify="space-between" wrap="nowrap">
                                                            <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden' }}>
                                                                <IconFile size={16} />
                                                                <Text size="sm" style={{ wordBreak: 'break-all' }}>{p}</Text>
                                                                {pageCount != null && (
                                                                    <Badge size="sm" variant="light" color={isHeavy ? 'orange' : 'gray'}>
                                                                        {pageCount}p
                                                                    </Badge>
                                                                )}
                                                            </Group>
                                                            <Group gap="xs" wrap="nowrap">
                                                                <Button
                                                                    size="compact-xs"
                                                                    variant={opts.crop ? 'filled' : 'light'}
                                                                    leftSection={<IconCrop size={14} />}
                                                                    onClick={(e) => {
                                                                        e.stopPropagation()
                                                                        setCropTarget(p)
                                                                    }}
                                                                >
                                                                    トリミング
                                                                </Button>
                                                            </Group>
                                                        </Group>
                                                        {isPdf && (
                                                            <Group grow>
                                                                <NumberInput
                                                                    size="xs"
                                                                    placeholder="開始"
                                                                    label="開始ページ"
                                                                    min={1}
                                                                    value={opts.start}
                                                                    onChange={(v) => {
                                                                        const val = typeof v === 'number' ? v : undefined;
                                                                        setOptions(prev => ({
                                                                            ...prev,
                                                                            fileOptions: {
                                                                                ...prev.fileOptions,
                                                                                [p]: { ...prev.fileOptions[p], start: val }
                                                                            }
                                                                        }));
                                                                    }}
                                                                />
                                                                <NumberInput
                                                                    size="xs"
                                                                    placeholder="終了"
                                                                    label="終了ページ"
                                                                    min={1}
                                                                    value={opts.end}
                                                                    onChange={(v) => {
                                                                        const val = typeof v === 'number' ? v : undefined;
                                                                        setOptions(prev => ({
                                                                            ...prev,
                                                                            fileOptions: {
                                                                                ...prev.fileOptions,
                                                                                [p]: { ...prev.fileOptions[p], end: val }
                                                                            }
                                                                        }));
                                                                    }}
                                                                />
                                                            </Group>
                                                        )}
                                                    </Stack>
                                                </Card>
                                            );
                                        })}
                                    </Stack>
                                </Stack>
                            )}
                        </Card>
                    </Stack>

                    <CropModal
                        opened={Boolean(cropTarget)}
                        filePath={cropTarget || ''}
                        initialCrop={cropTarget ? options.fileOptions[cropTarget]?.crop : undefined}
                        previewQuality={previewQuality}
                        onClose={() => setCropTarget(null)}
                        onSave={(crop) => {
                            const p = cropTarget
                            if (!p) return
                            setOptions(prev => ({
                                ...prev,
                                fileOptions: {
                                    ...prev.fileOptions,
                                    [p]: { ...prev.fileOptions[p], crop }
                                }
                            }))
                        }}
                    />

                    {/* ページ数警告 */}
                    {Object.values(pageCountMap).some((c) => c > 50) && (
                        <Alert icon={<IconAlertTriangle size={16} />} color="orange" variant="light">
                            ページ数が多いファイルがあります。低スペックPCでは「超省エネ (100 DPI)」＋「ページ間休止 ON」を推奨します。
                        </Alert>
                    )}

                    {/* Right Column: Options & Action */}
                    <Stack gap="lg">
                        <Button
                            size="lg"
                            radius="md"
                            fullWidth
                            disabled={filePaths.length === 0 || status === 'running'}
                            loading={status === 'running'}
                            leftSection={<IconPlayerPlay size={20} />}
                            onClick={onRun}
                            color="blue"
                        >
                            処理を実行
                        </Button>

                        {/* Status Section */}
                        {(status !== 'idle' || error) && (
                            <Card withBorder shadow="sm" radius="lg" padding="lg">
                                <Stack gap="md">
                                    <Group justify="space-between">
                                        <Group gap="xs">
                                            <Text fw={600}>ステータス:</Text>
                                            <Badge
                                                size="lg"
                                                variant="light"
                                                color={status === 'done' ? 'green' : status === 'running' ? 'blue' : 'gray'}
                                            >
                                                {status === 'running' ? '処理中...' : status === 'done' ? '完了' : '待機中'}
                                            </Badge>
                                        </Group>
                                        {status === 'running' && <Text size="sm" fw={700} c="blue">{Math.round(progress)}%</Text>}
                                    </Group>

                                    <Progress
                                        value={status === 'done' ? 100 : status === 'running' ? progress : 0}
                                        animated={status === 'running'}
                                        size="lg"
                                        radius="xl"
                                        styles={{
                                            section: { transition: 'width 0.3s ease' }
                                        }}
                                    />

                                    {status === 'running' && (currentMessage || etaSeconds != null) && (
                                        <Text size="sm" c="dimmed">
                                            {currentMessage || '処理中'}{etaSeconds != null ? `（残り約 ${formatEta(etaSeconds)}）` : ''}
                                        </Text>
                                    )}

                                    {error && (
                                        <Alert icon={<IconX size={16} />} title="エラーが発生しました" color="red" variant="light">
                                            {error}
                                        </Alert>
                                    )}

                                    {log.length > 0 && (
                                        <Box mt="sm">
                                            <Text size="xs" fw={600} mb={4} c="dimmed">ログ出力</Text>
                                            <Box
                                                ref={logBoxRef}
                                                bg="dark.8"
                                                c="gray.3"
                                                p="xs"
                                                style={{ borderRadius: 8, maxHeight: 150, overflowY: 'auto', fontFamily: 'monospace', fontSize: 12 }}
                                            >
                                                {log.slice(-10).map((line, i) => (
                                                    <div key={i}>{line}</div>
                                                ))}
                                            </Box>
                                        </Box>
                                    )}
                                </Stack>
                            </Card>
                        )}

                        <Card withBorder shadow="sm" radius="lg" padding="lg">
                            <Group justify="space-between" align="center" mb="sm">
                                <Text fw={600} size="sm" c="dimmed" tt="uppercase" style={{ letterSpacing: '0.5px' }}>
                                    オプション
                                </Text>
                                <Button
                                    size="xs"
                                    variant="light"
                                    leftSection={<IconDeviceFloppy size={16} />}
                                    loading={savingDefaults}
                                    onClick={handleSaveDefaults}
                                >
                                    デフォルトに保存
                                </Button>
                            </Group>

                            <Stack gap="md">
                                <CheckboxGroup
                                    label={<Text size="sm" fw={500} mb={4}>出力形式</Text>}
                                    value={options.formats}
                                    onChange={(v) => setOptions((prev) => ({ ...prev, formats: v }))}
                                >
                                    <Group mt="xs">
                                        <Checkbox value="md" label="Markdown" />
                                        <Checkbox value="docx" label="Word (docx)" />
                                        <Checkbox value="xlsx" label="Excel (xlsx)" />
                                        <Checkbox value="csv" label="CSV" />
                                    </Group>
                                </CheckboxGroup>

                                {hasEligiblePdfText && (
                                    <Switch
                                        label="埋め込みテキストを使用する（OCRスキップ）"
                                        description="選択したPDFにテキストが埋め込まれていることを検出しました。有効にすると、そのページはOCRを行わずPDF内のテキストをそのまま使用します（高速）。デフォルトはOFF（通常通りOCR）です。既存テキストの品質は保証されないため、以前スキャナ等で簡易OCR済みのPDFでは文字の誤りが残る場合があります。"
                                        checked={options.useEmbeddedText}
                                        onChange={(e) =>
                                            setOptions((prev) => ({ ...prev, useEmbeddedText: e.currentTarget.checked }))
                                        }
                                    />
                                )}

                                <Switch
                                    label="図表抽出（YOLOv8）"
                                    description="図・表を自動検出して埋め込みます。CPU 負荷が高くなります。"
                                    checked={options.enableFigure}
                                    onChange={(e) =>
                                        setOptions((prev) => ({ ...prev, enableFigure: e.currentTarget.checked }))
                                    }
                                />

                                <Switch
                                    label="ページ間休止"
                                    description="1ページごとに CPU を冷ます休止を挟みます。MBA など発熱が気になる場合は ON にしてください。"
                                    checked={options.enableRest}
                                    onChange={(e) =>
                                        setOptions((prev) => ({ ...prev, enableRest: e.currentTarget.checked }))
                                    }
                                />
                                {options.enableRest && (
                                    <NumberInput
                                        label="休止秒数"
                                        min={1}
                                        max={60}
                                        value={options.restSeconds}
                                        onChange={(v) => {
                                            const val = typeof v === 'number' ? v : 5
                                            setOptions((prev) => ({ ...prev, restSeconds: val }))
                                        }}
                                        style={{ maxWidth: 160 }}
                                    />
                                )}

                                {options.formats.includes('docx') && (
                                    <Stack gap="xs">
                                        <Text size="sm" fw={500}>Word出力方式</Text>
                                        <Radio.Group
                                            value={options.docxEngine}
                                            onChange={(val) =>
                                                setOptions((prev) => ({
                                                    ...prev,
                                                    docxEngine: val as 'python' | 'pandoc',
                                                }))
                                            }
                                        >
                                            <Stack gap="xs">
                                                <Radio value="python" label="標準（python-docx）" />
                                                <Radio value="pandoc" label="数式ブロック（Pandoc）" />
                                            </Stack>
                                        </Radio.Group>
                                        <Text size="xs" c="dimmed">
                                            Pandoc が必要です。数式ブロック化は md 入力が前提です。
                                        </Text>
                                    </Stack>
                                )}

                                {(options.formats.includes('xlsx') || options.formats.includes('csv')) && (
                                    <Stack gap="xs">
                                        <Text size="sm" fw={500}>表出力モード</Text>
                                        <SegmentedControl
                                            value={options.excelMode}
                                            onChange={(val) =>
                                                setOptions((prev) => ({
                                                    ...prev,
                                                    excelMode: val as 'layout' | 'table',
                                                }))
                                            }
                                            data={[
                                                { label: '通常（レイアウト）', value: 'layout' },
                                                { label: 'テーブル（結合解除）', value: 'table' },
                                            ]}
                                        />
                                        <Text size="xs" c="dimmed">
                                            テーブルモードはセル結合を解除し、構造変化ごとに分割します。
                                        </Text>
                                    </Stack>
                                )}

                                <Divider />

                                <Stack gap="xs">
                                    <Text size="sm" fw={500}>PDF DPI</Text>
                                    <Text size="xs" c="dimmed">
                                        PDF 入力時の変換 DPI です。保存しない場合はアプリ終了までの一時設定になります。
                                    </Text>
                                    <SegmentedControl
                                        value={dpiPreset}
                                        onChange={(v) => {
                                            setDpiPreset(v as typeof dpiPreset)
                                            if (v === 'custom') return
                                            const parsed = Number(v)
                                            if (!Number.isFinite(parsed)) return
                                            setOptions((prev) => ({ ...prev, pdfDpi: parsed }))
                                        }}
                                        data={[
                                            { label: '超省エネ (100)', value: '100' },
                                            { label: '省エネ (150)', value: '150' },
                                            { label: '標準 (200)', value: '200' },
                                            { label: '高精細 (300)', value: '300' },
                                        ]}
                                    />
                                </Stack>

                            </Stack>
                        </Card>
                    </Stack>
                </SimpleGrid>

                {/* Status Section */}

            </Stack>
        </Container>
    )
}
