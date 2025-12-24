import {
    Stack,
    Title,
    Text,
    Card,
    Group,
    Button,
    Badge,
    ScrollArea,
    ActionIcon,
    Tooltip,
    Container,
    SegmentedControl,
} from '@mantine/core'
import { IconDownload, IconCopy, IconCheck, IconExternalLink, IconFolderOpen } from '@tabler/icons-react'
import { useEffect, useMemo, useState } from 'react'
import { save } from '@tauri-apps/plugin-dialog'
import { openOutput, openOutputDir, saveFile } from '../api/runJob'
import { notifications } from '@mantine/notifications'

interface ResultProps {
    outputs: string[]
    resultText: string
    error: string | null
    jobId?: string | null
}

export function Result({ outputs, resultText, error, jobId }: ResultProps) {
    const [copied, setCopied] = useState(false)
    const [saving, setSaving] = useState(false)
    const [opening, setOpening] = useState(false)
    const [selectedFile, setSelectedFile] = useState<string | null>(null)
    const [previewMode, setPreviewMode] = useState<'markdown' | 'plain'>('markdown')

    const toPlainText = (md: string) => {
        let text = md || ''
        // code fences: keep contents, drop markers
        text = text.replace(/```[^\n]*\n/g, '')
        text = text.replace(/```/g, '')
        // images: ![alt](url) -> alt
        text = text.replace(/!\[([^\]]*)\]\([^\)]*\)/g, '$1')
        // links: [text](url) -> text
        text = text.replace(/\[([^\]]+)\]\([^\)]*\)/g, '$1')
        // headings: "# Title" -> "Title"
        text = text.replace(/^\s{0,3}#{1,6}\s+/gm, '')
        // blockquote marker
        text = text.replace(/^\s{0,3}>\s?/gm, '')
        // horizontal rules
        text = text.replace(/^\s{0,3}(-{3,}|\*{3,}|_{3,})\s*$/gm, '')
        // table separator lines (e.g. |---|---|)
        text = text.replace(/^\s*\|?(?:\s*:?[-]{2,}:?\s*\|)+\s*:?[-]{2,}:?\s*\|?\s*$/gm, '')
        // inline code
        text = text.replace(/`([^`]+)`/g, '$1')
        // emphasis markers
        text = text.replace(/\*\*([^*]+)\*\*/g, '$1')
        text = text.replace(/__([^_]+)__/g, '$1')
        text = text.replace(/\*([^*]+)\*/g, '$1')
        text = text.replace(/_([^_]+)_/g, '$1')
        // unordered list markers
        text = text.replace(/^\s*[-*+]\s+/gm, '• ')
        // ordered list markers
        text = text.replace(/^\s*\d+\.\s+/gm, '')
        // html tags
        text = text.replace(/<\/?[^>]+>/g, '')
        // normalize blank lines
        text = text.replace(/\n{3,}/g, '\n\n')
        return text.trim()
    }

    const previewText = useMemo(() => {
        if (!resultText) return ''
        return previewMode === 'plain' ? toPlainText(resultText) : resultText
    }, [resultText, previewMode])

    const handleCopy = () => {
        navigator.clipboard.writeText(previewText)
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
    }

    const selectedExt = useMemo(() => {
        if (!selectedFile) return null
        const i = selectedFile.lastIndexOf('.')
        if (i < 0) return null
        return selectedFile.slice(i + 1).toLowerCase()
    }, [selectedFile])

    useEffect(() => {
        if (!outputs.length) {
            setSelectedFile(null)
            return
        }
        if (selectedFile && outputs.includes(selectedFile)) return
        const priority = ['docx', 'xlsx', 'csv', 'md']
        const picked = priority
            .map((ext) => outputs.find((o) => o.toLowerCase().endsWith(`.${ext}`)))
            .find(Boolean)
        setSelectedFile(picked ?? outputs[0])
    }, [outputs, selectedFile])

    const handleOpenFile = async () => {
        if (!jobId) {
            notifications.show({ title: 'エラー', message: 'Job ID missing', color: 'red' })
            return
        }
        if (!selectedFile) {
            notifications.show({
                title: 'エラー',
                message: '開くファイルが選択されていません',
                color: 'red',
            })
            return
        }
        try {
            setOpening(true)
            await openOutput(jobId, selectedFile)
        } catch (e) {
            const msg = String(e)
            if (msg.includes('invoke') || msg.includes('undefined')) {
                notifications.show({
                    title: '環境エラー',
                    message: 'Tauri 環境外では開けません。Tauri アプリで実行してください。',
                    color: 'red',
                })
            } else {
                notifications.show({ title: '起動エラー', message: msg, color: 'red' })
            }
        } finally {
            setOpening(false)
        }
    }

    const handleSaveSelected = async () => {
        if (!jobId) {
            notifications.show({ title: 'エラー', message: 'Job ID missing', color: 'red' })
            return
        }
        if (!selectedFile) {
            notifications.show({
                title: 'エラー',
                message: '保存するファイルが選択されていません',
                color: 'red',
            })
            return
        }
        try {
            const filters = selectedExt
                ? [
                    {
                        name: selectedExt.toUpperCase(),
                        extensions: [selectedExt],
                    },
                ]
                : undefined

            const savePath = await save({
                defaultPath: selectedFile,
                filters,
            })

            if (!savePath) return

            setSaving(true)
            await saveFile(jobId, selectedFile, savePath)
            notifications.show({
                title: '保存完了',
                message: `ファイルを保存しました: ${savePath}`,
                color: 'green',
            })
        } catch (e) {
            const msg = String(e)
            if (msg.includes('invoke') || msg.includes('undefined')) {
                notifications.show({
                    title: '環境エラー',
                    message: 'Tauri 環境外では保存できません。Tauri アプリで実行してください。',
                    color: 'red',
                })
            } else {
                notifications.show({
                    title: '保存エラー',
                    message: msg,
                    color: 'red',
                })
            }
        } finally {
            setSaving(false)
        }
    }

    const handleOpenFolder = async () => {
        if (!jobId) {
            notifications.show({ title: 'エラー', message: 'Job ID missing', color: 'red' })
            return
        }
        try {
            setOpening(true)
            await openOutputDir(jobId)
        } catch (e) {
            const msg = String(e)
            if (msg.includes('invoke') || msg.includes('undefined')) {
                notifications.show({
                    title: '環境エラー',
                    message: 'Tauri 環境外では開けません。Tauri アプリで実行してください。',
                    color: 'red',
                })
            } else {
                notifications.show({ title: '起動エラー', message: msg, color: 'red' })
            }
        } finally {
            setOpening(false)
        }
    }

    return (
        <Container size="lg" px={0}>
            <Stack gap="xl">
                <Stack gap={4}>
                    <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                        結果プレビュー
                    </Title>
                    <Text c="dimmed">処理結果の確認とダウンロードができます。</Text>
                </Stack>

                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Stack gap="lg">
                        <div>
                            <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                                生成されたファイル
                            </Text>
                            <Group gap="xs">
                                {outputs.length === 0 ? (
                                    <Badge color="gray" variant="light" size="lg">なし</Badge>
                                ) : (
                                    outputs.map((o) => (
                                        <Badge
                                            key={o}
                                            component="button"
                                            type="button"
                                            color={selectedFile === o ? 'blue' : 'gray'}
                                            variant={selectedFile === o ? 'filled' : 'light'}
                                            size="lg"
                                            radius="sm"
                                            onClick={() => setSelectedFile(o)}
                                            style={{ cursor: 'pointer' }}
                                        >
                                            {o}
                                        </Badge>
                                    ))
                                )}
                            </Group>
                        </div>

                        <div>
                            <Group justify="space-between" mb="xs">
                                <Text fw={600} size="sm" c="dimmed" tt="uppercase" style={{ letterSpacing: '0.5px' }}>
                                    テキストプレビュー
                                </Text>
                                <Group gap="xs">
                                    <SegmentedControl
                                        value={previewMode}
                                        onChange={(v) => setPreviewMode(v as 'markdown' | 'plain')}
                                        data={[
                                            { label: 'Plain', value: 'plain' },
                                            { label: 'Markdown', value: 'markdown' },
                                        ]}
                                    />
                                    <Tooltip label={copied ? "コピーしました" : "クリップボードにコピー"} withArrow>
                                        <ActionIcon variant="subtle" color={copied ? 'teal' : 'gray'} onClick={handleCopy}>
                                            {copied ? <IconCheck size={18} /> : <IconCopy size={18} />}
                                        </ActionIcon>
                                    </Tooltip>
                                </Group>
                            </Group>

                            <Card withBorder radius="md" bg="gray.0" p={0}>
                                <ScrollArea h={400} p="md">
                                    {error ? (
                                        <Text c="red">{error}</Text>
                                    ) : previewText ? (
                                        <Text size="sm" style={{ whiteSpace: 'pre-wrap', fontFamily: 'monospace' }}>
                                            {previewText}
                                        </Text>
                                    ) : (
                                        <Text c="dimmed" size="sm" fs="italic">
                                            プレビューはまだありません。
                                        </Text>
                                    )}
                                </ScrollArea>
                            </Card>
                        </div>
                    </Stack>
                </Card>

                <Group>
                    <Button
                        leftSection={<IconFolderOpen size={18} />}
                        variant="light"
                        color="gray"
                        onClick={handleOpenFolder}
                        loading={opening}
                        disabled={!jobId}
                    >
                        結果フォルダを開く
                    </Button>
                    <Button
                        leftSection={<IconExternalLink size={18} />}
                        variant="light"
                        color="blue"
                        onClick={handleOpenFile}
                        loading={opening || saving}
                        disabled={!jobId || !selectedFile}
                    >
                        ファイルを開く
                    </Button>
                    <Button
                        leftSection={<IconDownload size={18} />}
                        variant="filled"
                        color="blue"
                        onClick={handleSaveSelected}
                        loading={saving || opening}
                        disabled={!jobId || !selectedFile}
                    >
                        ファイルを保存
                    </Button>
                </Group>
            </Stack>
        </Container>
    )
}
