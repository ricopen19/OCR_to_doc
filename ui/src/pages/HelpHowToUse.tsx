import { Card, Stack, Text, Title } from '@mantine/core'

import howToUseRaw from '../assets/how_to_use.md?raw'
import { HelpTextBlocks } from '../components/Help/HelpTextBlocks'

export function HelpHowToUse() {
    return (
        <Stack gap="xl">
            <Stack gap={4}>
                <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                    このアプリの使い方
                </Title>
                <Text c="dimmed">基本的な使い方と注意点をまとめています。</Text>
            </Stack>

            <Card withBorder radius="lg" padding="lg">
                <HelpTextBlocks raw={howToUseRaw} />
            </Card>
        </Stack>
    )
}

