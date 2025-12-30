import { useEffect, useMemo, useState } from 'react'
import { Card, Group, Stack, Text, Title } from '@mantine/core'
import { getName, getTauriVersion, getVersion } from '@tauri-apps/api/app'

import aboutMarkdownRaw from '../assets/about.md?raw'
import { HelpTextBlocks } from '../components/Help/HelpTextBlocks'

type AppInfo = {
    appName: string
    appVersion: string
    tauriVersion: string
    buildNumber: string
    gitSha: string
}

function detectTauri(): boolean {
    return typeof window !== 'undefined' && ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
}

export function HelpAbout() {
    const [info, setInfo] = useState<AppInfo>({
        appName: 'OCR to Doc',
        appVersion: 'unknown',
        tauriVersion: 'unknown',
        buildNumber: import.meta.env.VITE_BUILD_NUMBER ?? 'unknown',
        gitSha: import.meta.env.VITE_GIT_SHA ?? 'unknown',
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
                        <Text fw={600}>ビルド番号</Text>
                        <Text>{info.buildNumber}</Text>
                    </Group>
                    <Group justify="space-between" wrap="wrap" gap="xs">
                        <Text fw={600}>コミット</Text>
                        <Text>{info.gitSha}</Text>
                    </Group>
                    <Group justify="space-between" wrap="wrap" gap="xs">
                        <Text fw={600}>Tauri</Text>
                        <Text>{info.tauriVersion}</Text>
                    </Group>
                </Stack>
            </Card>

            <Card withBorder radius="lg" padding="lg">
                <Stack gap="sm">
                    <Title order={4}>詳細</Title>
                    <Text c="dimmed">内容は `ui/src/assets/about.md` を編集して更新できます。</Text>
                    <HelpTextBlocks raw={aboutMarkdownRaw} />
                </Stack>
            </Card>
        </Stack>
    )
}
