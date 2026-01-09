import { useEffect, useMemo, useState } from 'react'
import { Card, Group, Stack, Text, Title } from '@mantine/core'
import { getName, getTauriVersion, getVersion } from '@tauri-apps/api/app'
import { notifications } from '@mantine/notifications'

import aboutMarkdownRaw from '../assets/about.md?raw'
import { openReadme } from '../api/readme'
import { HelpTextBlocks } from '../components/Help/HelpTextBlocks'

type AppInfo = {
    appName: string
    appVersion: string
    tauriVersion: string
}

function detectTauri(): boolean {
    return typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
}

export function HelpAbout() {
    const [info, setInfo] = useState<AppInfo>({
        appName: 'OCR to Doc',
        appVersion: 'unknown',
        tauriVersion: 'unknown',
    })

    const hasTauri = useMemo(() => detectTauri(), [])

    useEffect(() => {
        if (!hasTauri) return
        Promise.allSettled([getName(), getVersion(), getTauriVersion()]).then((results) => {
            const [name, version, tauri] = results
            setInfo((prev) => ({
                ...prev,
                appName: name.status === 'fulfilled' ? name.value : prev.appName,
                appVersion: version.status === 'fulfilled' ? version.value : prev.appVersion,
                tauriVersion: tauri.status === 'fulfilled' ? tauri.value : prev.tauriVersion,
            }))
        })
    }, [hasTauri])

    return (
        <Stack gap="xl">
            <Stack gap={4}>
                <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                    バージョン情報
                </Title>
                <Text c="dimmed">アプリの基本情報・連絡先・注意事項を表示します。</Text>
            </Stack>

            <Card withBorder radius="lg" padding="lg">
                <Stack gap="sm">
                    <Group justify="space-between" wrap="wrap" gap="xs">
                        <Text fw={600}>アプリ名</Text>
                        <Text>{info.appName}</Text>
                    </Group>
                    <Group justify="space-between" wrap="wrap" gap="xs">
                        <Text fw={600}>バージョン</Text>
                        <Text>{info.appVersion}</Text>
                    </Group>
                    <Group justify="space-between" wrap="wrap" gap="xs">
                        <Text fw={600}>Tauri</Text>
                        <Text>{info.tauriVersion}</Text>
                    </Group>
                </Stack>
            </Card>

            <Card withBorder radius="lg" padding="lg">
                <Stack gap="sm">
                    <HelpTextBlocks
                        raw={aboutMarkdownRaw}
                        onOpenLocalLink={async (href) => {
                            if (href !== 'app://readme') return
                            try {
                                await openReadme()
                            } catch (err) {
                                console.error(err)
                                notifications.show({
                                    title: 'エラー',
                                    message: 'README を開けませんでした',
                                    color: 'red',
                                })
                            }
                        }}
                    />
                </Stack>
            </Card>
        </Stack>
    )
}
