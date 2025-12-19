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
} from '@mantine/core'
import { IconPlayerPlay, IconHistory, IconRocket } from '@tabler/icons-react'

interface HomeProps {
    onNavigate: (page: 'run') => void
}

export function Home({ onNavigate }: HomeProps) {
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
                                <Text size="sm" c="dimmed">
                                    まだ処理履歴がありません。
                                </Text>
                            </div>
                            {/* Placeholder for future list */}
                        </Stack>
                    </Card>
                </SimpleGrid>
            </Stack>
        </Container>
    )
}
