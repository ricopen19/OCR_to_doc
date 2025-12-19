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
} from '@mantine/core'
import { IconDownload, IconCopy, IconCheck } from '@tabler/icons-react'
import { useState } from 'react'
import { save } from '@tauri-apps/plugin-dialog'
import { saveFile } from '../api/runJob'
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

    const handleCopy = () => {
        navigator.clipboard.writeText(resultText)
        setCopied(true)
        setTimeout(() => setCopied(false), 2000)
    }

    const handleDownload = async (ext: string) => {
        console.log('handleDownload called for:', ext, 'jobId:', jobId)
        if (!jobId) {
            console.error('jobId is missing')
            notifications.show({ title: 'Error', message: 'Job ID missing', color: 'red' })
            return
        }
        const targetFile = outputs.find(o => o.endsWith(`.${ext}`))
        console.log('targetFile:', targetFile)
        if (!targetFile) {
            notifications.show({
                title: 'エラー',
                message: `${ext} ファイルが見つかりません`,
                color: 'red',
            })
            return
        }

        try {
            console.log('Opening save dialog...')
            const savePath = await save({
                defaultPath: targetFile,
                filters: [{
                    name: ext.toUpperCase(),
                    extensions: [ext]
                }]
            })
            console.log('savePath:', savePath)

            if (!savePath) return // Canceled

            setSaving(true)
            await saveFile(jobId, targetFile, savePath)
            notifications.show({
                title: '保存完了',
                message: `ファイルを保存しました: ${savePath}`,
                color: 'green',
            })
        } catch (e) {
            console.error('Download error:', e)
            const msg = String(e)
            if (msg.includes('invoke') || msg.includes('undefined')) {
                notifications.show({
                    title: '環境エラー',
                    message: 'Tauri 環境外ではダウンロードできません。Tauri アプリで実行してください。',
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
                                        <Badge key={o} color="blue" variant="light" size="lg" radius="sm">
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
                                <Tooltip label={copied ? "コピーしました" : "クリップボードにコピー"} withArrow>
                                    <ActionIcon variant="subtle" color={copied ? 'teal' : 'gray'} onClick={handleCopy}>
                                        {copied ? <IconCheck size={18} /> : <IconCopy size={18} />}
                                    </ActionIcon>
                                </Tooltip>
                            </Group>

                            <Card withBorder radius="md" bg="gray.0" p={0}>
                                <ScrollArea h={400} p="md">
                                    {error ? (
                                        <Text c="red">{error}</Text>
                                    ) : resultText ? (
                                        <Text size="sm" style={{ whiteSpace: 'pre-wrap', fontFamily: 'monospace' }}>
                                            {resultText}
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
                        leftSection={<IconDownload size={18} />}
                        variant="filled"
                        color="blue"
                        onClick={() => handleDownload('md')}
                        loading={saving}
                        disabled={!outputs.some(o => o.endsWith('.md'))}
                    >
                        Markdown を保存
                    </Button>
                    <Button
                        leftSection={<IconDownload size={18} />}
                        variant="default"
                        onClick={() => handleDownload('docx')}
                        loading={saving}
                        disabled={!outputs.some(o => o.endsWith('.docx'))}
                    >
                        Word (docx) を保存
                    </Button>
                    <Button
                        leftSection={<IconDownload size={18} />}
                        variant="default"
                        onClick={() => handleDownload('xlsx')}
                        loading={saving}
                        disabled={!outputs.some(o => o.endsWith('.xlsx'))}
                    >
                        Excel (xlsx) を保存
                    </Button>
                </Group>
            </Stack>
        </Container>
    )
}
