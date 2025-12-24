import {
    Stack,
    Title,
    Text,
    Card,
    Group,
    Button,
    ThemeIcon,
    SimpleGrid,
    Container,
    ActionIcon,
    Tooltip,
    Badge,
    Divider,
} from '@mantine/core'
import {
    IconPlayerPlay,
    IconHistory,
    IconRocket,
    IconFolderOpen,
    IconExternalLink,
    IconCircleCheck,
    IconAlertTriangle,
    IconChevronDown,
    IconChevronUp,
} from '@tabler/icons-react'
import { useEffect, useState } from 'react'
import { checkEnvironment, listRecentResults, openResultDir, openResultFile, type EnvironmentStatus, type RecentResultEntry } from '../api/history'

interface HomeProps {
    onNavigate: (page: 'run') => void
}

export function Home({ onNavigate }: HomeProps) {
    const [recent, setRecent] = useState<RecentResultEntry[]>([])
    const [loadingRecent, setLoadingRecent] = useState(false)
    const [showAllHistory, setShowAllHistory] = useState(false)
    const [env, setEnv] = useState<EnvironmentStatus | null>(null)
    const hasTauri = typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)

    useEffect(() => {
        if (!hasTauri) {
            setRecent([])
            setEnv(null)
            return
        }
        setLoadingRecent(true)
        listRecentResults(10)
            .then((items) => setRecent(items))
            .catch((e) => {
                console.error(e)
                setRecent([])
            })
            .finally(() => setLoadingRecent(false))

        checkEnvironment()
            .then(setEnv)
            .catch((e) => {
                console.error(e)
                setEnv(null)
            })
    }, [hasTauri])

    const formatUpdated = (ms: number) => {
        try {
            const d = new Date(ms)
            const yyyy = d.getFullYear()
            const MM = String(d.getMonth() + 1).padStart(2, '0')
            const dd = String(d.getDate()).padStart(2, '0')
            const hh = String(d.getHours()).padStart(2, '0')
            const mm = String(d.getMinutes()).padStart(2, '0')
            return `${yyyy}/${MM}/${dd} ${hh}:${mm}`
        } catch {
            return ''
        }
    }

    const getFileTypeInfo = (path: string | undefined) => {
        if (!path) return { label: 'FILE', color: 'gray' }
        const ext = path.split('.').pop()?.toLowerCase()
        if (ext === 'md') return { label: 'Markdown', color: 'gray' }
        if (ext === 'docx') return { label: 'Word', color: 'blue' }
        if (ext === 'xlsx') return { label: 'Excel', color: 'green' }
        if (ext === 'csv') return { label: 'CSV', color: 'teal' }
        return { label: ext?.toUpperCase() || 'FILE', color: 'gray' }
    }

    const visibleRecent = showAllHistory ? recent : recent.slice(0, 3)

    return (
        <Container size="lg" px={0}>
            <Stack gap="xl">
                <Stack gap={4}>
                    <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                        ホーム
                    </Title>
                    <Text c="dimmed">OCR to Doc へようこそ。作業を開始しましょう。</Text>
                </Stack>

                <SimpleGrid cols={{ base: 1, md: 2 }} spacing="lg">
                    {/* Quick Start Card */}
                    <Card
                        padding="xl"
                        radius="lg"
                        withBorder
                        style={{
                            borderColor: 'var(--mantine-color-blue-1)',
                            backgroundColor: 'var(--mantine-color-blue-0)',
                        }}
                    >
                        <Stack justify="space-between" h="100%">
                            <div>
                                <Group mb="md">
                                    <ThemeIcon size="lg" radius="md" color="blue" variant="light">
                                        <IconRocket size={20} />
                                    </ThemeIcon>
                                    <Text fw={600} size="lg">クイック開始</Text>
                                </Group>
                                <Text size="sm" c="dimmed" lh={1.6}>
                                    新しいファイルをアップロードして、OCR処理とドキュメント変換をすぐに開始します。
                                </Text>
                            </div>
                            <Button
                                fullWidth
                                mt="xl"
                                size="md"
                                rightSection={<IconPlayerPlay size={16} />}
                                onClick={() => onNavigate('run')}
                            >
                                新規処理を開始
                            </Button>
                        </Stack>
                    </Card>

                    {/* Recent History Card */}
                    <Card padding="xl" radius="lg" withBorder>
                        <Stack justify="space-between" h="100%">
                            <div>
                                <Group mb="md">
                                    <ThemeIcon size="lg" radius="md" color="gray" variant="light">
                                        <IconHistory size={20} />
                                    </ThemeIcon>
                                    <Text fw={600} size="lg">最近の処理</Text>
                                </Group>
                                {loadingRecent ? (
                                    <Text size="sm" c="dimmed">
                                        読み込み中...
                                    </Text>
                                ) : !hasTauri ? (
                                    <Text size="sm" c="dimmed">
                                        最近の処理は Tauri アプリで表示されます。
                                    </Text>
                                ) : recent.length === 0 ? (
                                    <Text size="sm" c="dimmed">
                                        まだ処理履歴がありません。
                                    </Text>
                                ) : (
                                    <Stack gap="xs">
                                        {visibleRecent.map((item) => {
                                            const typeInfo = getFileTypeInfo(item.bestFile)
                                            return (
                                                <Card key={item.dirName} withBorder radius="md" padding="sm" bg="gray.0">
                                                    <Group justify="space-between" align="center" wrap="nowrap">
                                                        <Stack gap={2} style={{ minWidth: 0 }}>
                                                            <Group gap="xs" wrap="wrap">
                                                                <Text fw={600} size="sm" style={{ wordBreak: 'break-all' }}>
                                                                    {item.dirName}
                                                                </Text>
                                                                <Badge size="xs" variant="filled" color={typeInfo.color}>
                                                                    {typeInfo.label}
                                                                </Badge>
                                                                {item.pageRange && (
                                                                    <Badge size="xs" variant="outline" color="blue">
                                                                        {item.pageRange}
                                                                    </Badge>
                                                                )}
                                                            </Group>
                                                            <Text size="xs" c="dimmed">
                                                                {formatUpdated(item.updatedAtMs)}
                                                            </Text>
                                                        </Stack>

                                                        <Group gap={6} wrap="nowrap">
                                                            <Tooltip label="フォルダを開く" withArrow>
                                                                <ActionIcon
                                                                    variant="light"
                                                                    color="gray"
                                                                    onClick={() => openResultDir(item.dirName)}
                                                                >
                                                                    <IconFolderOpen size={16} />
                                                                </ActionIcon>
                                                            </Tooltip>
                                                            <Tooltip label="結果を開く" withArrow>
                                                                <ActionIcon
                                                                    variant="light"
                                                                    color="blue"
                                                                    disabled={!item.bestFile}
                                                                    onClick={() => openResultFile(item.dirName)}
                                                                >
                                                                    <IconExternalLink size={16} />
                                                                </ActionIcon>
                                                            </Tooltip>
                                                        </Group>
                                                    </Group>
                                                </Card>
                                            )
                                        })}
                                        {recent.length > 3 && (
                                            <Button
                                                variant="subtle"
                                                size="sm"
                                                color="gray"
                                                fullWidth
                                                onClick={() => setShowAllHistory(!showAllHistory)}
                                                leftSection={showAllHistory ? <IconChevronUp size={16} /> : <IconChevronDown size={16} />}
                                            >
                                                {showAllHistory ? '閉じる' : 'もっと見る'}
                                            </Button>
                                        )}
                                    </Stack>
                                )}
                            </div>
                        </Stack>
                    </Card>
                </SimpleGrid>

                <Card withBorder radius="lg" padding="xl">
                    <Stack gap="md">
                        <Group>
                            <ThemeIcon size="lg" radius="md" color={env && env.dispatcherFound && env.resultDirFound ? 'green' : 'yellow'} variant="light">
                                {env && env.dispatcherFound && env.resultDirFound ? <IconCircleCheck size={20} /> : <IconAlertTriangle size={20} />}
                            </ThemeIcon>
                            <div>
                                <Text fw={600} size="lg">環境チェック</Text>
                                <Text size="sm" c="dimmed">実行に必要な要素が見つかるかを確認します。</Text>
                            </div>
                        </Group>

                        <Divider />

                        {!hasTauri ? (
                            <Text size="sm" c="dimmed">
                                環境チェックは Tauri アプリで実行できます。
                            </Text>
                        ) : (
                            <Stack gap="xs">
                                <Group justify="space-between">
                                    <Text size="sm">dispatcher.py</Text>
                                    <Badge color={env?.dispatcherFound ? 'green' : 'red'} variant="light">
                                        {env?.dispatcherFound ? 'OK' : 'NG'}
                                    </Badge>
                                </Group>
                                <Group justify="space-between">
                                    <Text size="sm">result フォルダ</Text>
                                    <Badge color={env?.resultDirFound ? 'green' : 'red'} variant="light">
                                        {env?.resultDirFound ? 'OK' : 'NG'}
                                    </Badge>
                                </Group>
                                <Group justify="space-between">
                                    <Text size="sm">Python</Text>
                                    <Badge color={env?.pythonBin ? 'blue' : 'gray'} variant="light">
                                        {env?.pythonBin ? env.pythonBin : 'unknown'}
                                    </Badge>
                                </Group>
                            </Stack>
                        )}
                    </Stack>
                </Card>
            </Stack>
        </Container>
    )
}
