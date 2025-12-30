import { Anchor, Stack, Text } from '@mantine/core'

export type HelpTextBlock = {
    title: string
    body: string
}

function parseHelpTextBlocks(raw: string): HelpTextBlock[] {
    const normalized = raw.replace(/\r\n/g, '\n').trim()
    if (!normalized) return []

    // Backward compatibility: "Key: Value" format (1 line = 1 entry)
    const nonEmptyLines = normalized
        .split('\n')
        .map((line) => line.trimEnd())
        .filter((line) => line.trim().length > 0)
    const allColonPairs = nonEmptyLines.every((line) => /^([^:：]+)[：:]\s*(.*)$/.test(line))
    if (allColonPairs) {
        return nonEmptyLines
            .map((line) => {
                const match = /^([^:：]+)[：:]\s*(.*)$/.exec(line)
                return {
                    title: match?.[1]?.trim() ?? '',
                    body: (match?.[2] ?? '').trim(),
                }
            })
            .filter((e) => e.title.length > 0)
    }

    // Block format: blank-line separated, first line = title, rest = body
    return normalized
        .split(/\n\s*\n+/g)
        .map((block) => block.trim())
        .filter(Boolean)
        .map((block) => {
            const lines = block.split('\n').map((line) => line.trimEnd())
            const title = (lines[0] ?? '').trim()
            const body = lines.slice(1).join('\n').trim()
            return { title, body }
        })
        .filter((e) => e.title.length > 0)
}

function renderTextWithLinks(text: string) {
    const parts = text.split(/(https?:\/\/\S+)/g)
    return parts.map((part, idx) => {
        if (/^https?:\/\/\S+$/.test(part)) {
            return (
                <Anchor key={idx} href={part} target="_blank" rel="noreferrer">
                    {part}
                </Anchor>
            )
        }
        return <span key={idx}>{part}</span>
    })
}

export function HelpTextBlocks({ raw }: { raw: string }) {
    const blocks = parseHelpTextBlocks(raw)
    return (
        <Stack gap="md">
            {blocks.map((block, idx) => (
                <Stack key={idx} gap={4}>
                    <Text fw={700}>{block.title}</Text>
                    <Text style={{ whiteSpace: 'pre-wrap' }}>
                        {block.body ? renderTextWithLinks(block.body) : <Text span c="dimmed">（未設定）</Text>}
                    </Text>
                </Stack>
            ))}
        </Stack>
    )
}

