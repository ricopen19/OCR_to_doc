import {
    Stack,
    Title,
    Text,
    Card,
    SimpleGrid,
    Box,
    Button,
    List,
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
} from '@mantine/core'
import { IconUpload, IconPlayerPlay, IconFile, IconX } from '@tabler/icons-react'
import { useRef, type Dispatch, type SetStateAction } from 'react'

type FileWithPath = File & { path?: string }

export interface RunJobOptions {
    formats: string[]
    imageAsPdf: boolean
    enableFigure: boolean
}

interface RunJobProps {
    filePaths: string[]
    setFilePaths: (paths: string[]) => void
    status: 'idle' | 'running' | 'done'
    setStatus: (status: 'idle' | 'running' | 'done') => void
    progress: number
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
    log,
    error,
    setError,
    onRun,
    options,
    setOptions,
}: RunJobProps) {
    const fileInputRef = useRef<HTMLInputElement | null>(null)

    return (
        <Container size="lg" px={0}>
            <Stack gap="xl">
                <Stack gap={4}>
                    <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                        OCR 実行
                    </Title>
                    <Text c="dimmed">ファイルをアップロードして設定を選択してください。</Text>
                </Stack>

                <SimpleGrid cols={{ base: 1, md: 2 }} spacing="lg">
                    {/* Left Column: Input */}
                    <Stack gap="lg">
                        <Card withBorder shadow="sm" radius="lg" padding="lg">
                            <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                                入力ファイル
                            </Text>

                            <Box
                                onClick={() => fileInputRef.current?.click()}
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
                                <input
                                    ref={fileInputRef}
                                    type="file"
                                    multiple
                                    accept=".pdf,.heic,.jpg,.jpeg,.png"
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
                                            }}
                                        >
                                            クリア
                                        </Button>
                                    </Group>
                                    <Card withBorder radius="md" padding="xs" bg="gray.0">
                                        <List spacing={4} size="sm" center icon={<IconFile size={14} />}>
                                            {filePaths.map((p) => (
                                                <List.Item key={p} style={{ wordBreak: 'break-all' }}>
                                                    <Text span size="sm">{p}</Text>
                                                </List.Item>
                                            ))}
                                        </List>
                                    </Card>
                                </Stack>
                            )}
                        </Card>
                    </Stack>

                    {/* Right Column: Options & Action */}
                    <Stack gap="lg">
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
                                    </Group>
                                </CheckboxGroup>

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
                    </Stack>
                </SimpleGrid>

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
                                {status === 'running' && <Text size="sm" fw={700} c="blue">{Math.round(progress * 100)}%</Text>}
                            </Group>

                            <Progress
                                value={status === 'done' ? 100 : status === 'running' ? progress * 100 : 0}
                                animated={status === 'running'}
                                size="lg"
                                radius="xl"
                                styles={{
                                    section: { transition: 'width 0.3s ease' }
                                }}
                            />

                            {error && (
                                <Alert icon={<IconX size={16} />} title="エラーが発生しました" color="red" variant="light">
                                    {error}
                                </Alert>
                            )}

                            {log.length > 0 && (
                                <Box mt="sm">
                                    <Text size="xs" fw={600} mb={4} c="dimmed">ログ出力</Text>
                                    <Box
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
            </Stack>
        </Container>
    )
}
