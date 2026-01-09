import { Card, Stack, Text, Title } from '@mantine/core'

import troubleshootingRaw from '../assets/troubleshooting.md?raw'
import { HelpTextBlocks } from '../components/Help/HelpTextBlocks'

export function HelpTroubleshooting() {
    return (
        <Stack gap="xl">
            <Stack gap={4}>
                <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                    トラブルシューティング
                </Title>
                <Text c="dimmed">不具合や重さの対策をまとめています。</Text>
            </Stack>

            <Card withBorder radius="lg" padding="lg">
                <HelpTextBlocks raw={troubleshootingRaw} />
            </Card>
        </Stack>
    )
}
