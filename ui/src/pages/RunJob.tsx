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
    Switch,
    Group,
    Badge,
    Progress,
    Alert,
    Container,
    SegmentedControl,
    NumberInput,
} from '@mantine/core'
import { IconUpload, IconPlayerPlay, IconFile, IconX, IconAlertTriangle, IconCrop } from '@tabler/icons-react'
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import type { CropRect } from '../types/crop'
import { CropModal } from '../components/CropModal'
import { open } from '@tauri-apps/plugin-dialog'
import { listen } from '@tauri-apps/api/event'

type FileWithPath = File & { path?: string }

function hasTauriRuntime() {
    return typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
}

export interface RunJobOptions {
    formats: string[]
    imageAsPdf: boolean
    enableFigure: boolean
    useGpu: boolean
    mode: 'lite' | 'full'
    excelMode: 'layout' | 'table'
    chunkSize: number
    enableRest: boolean
    restSeconds: number
    pdfDpi: number
    fileOptions: Record<string, { start?: number; end?: number; crop?: CropRect }>
}

interface RunJobProps {
    filePaths: string[]
    setFilePaths: (paths: string[]) => void
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
}

export function RunJob({
    filePaths,
    setFilePaths,
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
}: RunJobProps) {
    const fileInputRef = useRef<HTMLInputElement | null>(null)
    const logBoxRef = useRef<HTMLDivElement | null>(null)
    const filePathsRef = useRef<string[]>([])
    const [cropTarget, setCropTarget] = useState<string | null>(null)
    const deriveDpiPreset = (dpi: number) =>
        ([200, 300, 400].includes(dpi) ? String(dpi) : 'custom') as '200' | '300' | '400' | 'custom'

    const [dpiPreset, setDpiPreset] = useState<'200' | '300' | '400' | 'custom'>(() =>
        deriveDpiPreset(options.pdfDpi ?? 300)
    )

    useEffect(() => {
        setDpiPreset(deriveDpiPreset(options.pdfDpi ?? 300))
    }, [options.pdfDpi])

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
                                    <Text fw={600} size="md">クリック または ドラッグ＆ドロップ</Text>
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
                                            return (
                                                <Card key={p} withBorder radius="md" padding="sm" bg="gray.0">
                                                    <Stack gap="xs">
                                                        <Group justify="space-between" wrap="nowrap">
                                                            <Group gap="xs" wrap="nowrap" style={{ overflow: 'hidden' }}>
                                                                <IconFile size={16} />
                                                                <Text size="sm" style={{ wordBreak: 'break-all' }}>{p}</Text>
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
                            <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                                オプション
                            </Text>

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
                                    <Text size="sm" fw={500}>処理モード</Text>
                                    <SegmentedControl
                                        value={options.mode}
                                        onChange={(val) => setOptions(prev => ({ ...prev, mode: val as 'lite' | 'full' }))}
                                        data={[
                                            { label: 'Lite (高速)', value: 'lite' },
                                            { label: 'Full (高精度)', value: 'full' },
                                        ]}
                                    />
                                    {options.mode === 'full' && (
                                        <Alert variant="light" color="yellow" title="注意" icon={<IconAlertTriangle size={16} />}>
                                            Fullモードは非常に高負荷で時間がかかります。通常はLite推奨です。
                                        </Alert>
                                    )}
                                </Stack>

                                <Divider />

                                <Stack gap="xs">
                                    <Text size="sm" fw={500}>PDF DPI（今回のみ）</Text>
                                    <Text size="xs" c="dimmed">
                                        設定画面のデフォルト値を一時的に上書きします（PDF入力時のみ有効）
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
                                            { label: '低 (200)', value: '200' },
                                            { label: '標準 (300)', value: '300' },
                                            { label: '高精細 (400)', value: '400' },
                                            { label: 'カスタム', value: 'custom' },
                                        ]}
                                    />
                                    {dpiPreset === 'custom' && (
                                        <NumberInput
                                            label="カスタム DPI"
                                            min={72}
                                            max={600}
                                            value={options.pdfDpi ?? 300}
                                            onChange={(v) => {
                                                const parsed = typeof v === 'number' ? v : null
                                                if (!parsed) return
                                                setOptions((prev) => ({ ...prev, pdfDpi: parsed }))
                                            }}
                                        />
                                    )}
                                </Stack>

                                <Divider />

                                <Switch
                                    label="画像をPDF化してから処理"
                                    description="複数の画像を1つのPDFとしてまとめて処理します"
                                    checked={options.imageAsPdf}
                                    onChange={() => setOptions((prev) => ({ ...prev, imageAsPdf: !prev.imageAsPdf }))}
                                />
                                <Switch
                                    label="図表抽出 (Experimental)"
                                    description="図表を画像として切り出します"
                                    checked={options.enableFigure}
                                    onChange={() => setOptions((prev) => ({ ...prev, enableFigure: !prev.enableFigure }))}
                                />
                            </Stack>
                        </Card>
                    </Stack>
                </SimpleGrid>

                {/* Status Section */}

            </Stack>
        </Container>
    )
}
