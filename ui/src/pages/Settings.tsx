
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useState } from 'react'
import {
    Stack,
    Title,
    Text,
    Card,
    Button,
    Group,
    TextInput,
    Checkbox,
    Switch,
    Divider,
    NumberInput,
    Collapse,
    SegmentedControl,
} from '@mantine/core'
import { IconDeviceFloppy, IconFolder } from '@tabler/icons-react'
import { open } from '@tauri-apps/plugin-dialog'
import { notifications } from '@mantine/notifications'
import { getCurrentWindowSize, type AppSettings, loadSettings, saveSettings } from '../api/settings'

export type SettingsHandle = {
    isDirty: () => boolean
    save: () => Promise<boolean>
}

function settingsSnapshot(settings: AppSettings): string {
    const formats = Array.isArray(settings.formats) ? [...settings.formats].sort() : []
    const outputRoot = settings.outputRoot?.trim()
    return JSON.stringify({
        ...settings,
        formats,
        outputRoot: outputRoot ? outputRoot : undefined,
    })
}

export const Settings = forwardRef<SettingsHandle>(function Settings(_props, ref) {
    const [settings, setSettings] = useState<AppSettings | null>(null)
    const [initialSnapshot, setInitialSnapshot] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const [dpiPreset, setDpiPreset] = useState<'200' | '300' | '400' | 'custom'>('300')

    useEffect(() => {
        loadSettings().then((s) => {
            setSettings(s)
            setInitialSnapshot(settingsSnapshot(s))
            const dpi = s.pdfDpi ?? 300
            const preset = [200, 300, 400].includes(dpi) ? String(dpi) : 'custom'
            setDpiPreset(preset as typeof dpiPreset)
        }).catch((err) => {
            console.error(err)
            notifications.show({
                title: 'エラー',
                message: '設定の読み込みに失敗しました',
                color: 'red',
            })
        })
    }, [])

    const isDirty = useMemo(() => {
        if (!settings) return false
        if (!initialSnapshot) return false
        return settingsSnapshot(settings) !== initialSnapshot
    }, [settings, initialSnapshot])

    const saveCurrent = useCallback(async (): Promise<boolean> => {
        if (!settings) return false
        setLoading(true)
        try {
            const outputRootTrimmed = settings.outputRoot?.trim()
            const toSave: AppSettings = {
                ...settings,
                outputRoot: outputRootTrimmed ? outputRootTrimmed : undefined,
            }
            await saveSettings(toSave)
            setSettings(toSave)
            setInitialSnapshot(settingsSnapshot(toSave))
            notifications.show({
                title: '保存しました',
                message: '設定を保存しました（次回起動時に反映されます）',
                color: 'green',
            })
            return true
        } catch (err) {
            console.error(err)
            notifications.show({
                title: 'エラー',
                message: '設定の保存に失敗しました: ' + String(err),
                color: 'red',
            })
            return false
        } finally {
            setLoading(false)
        }
    }, [settings])

    const handleSave = async () => {
        await saveCurrent()
    }

    useImperativeHandle(ref, () => ({
        isDirty: () => isDirty,
        save: () => saveCurrent(),
    }), [isDirty, saveCurrent])

    const handleBrowse = async () => {
        try {
            const selected = await open({
                directory: true,
                multiple: false,
            })
            if (selected && typeof selected === 'string') {
                setSettings((prev) => prev ? { ...prev, outputRoot: selected } : null)
            }
        } catch (err) {
            console.error(err)
        }
    }

    if (!settings) return <Text>設定を読み込み中...</Text>

    const dpiValue = settings.pdfDpi ?? 300

    return (
        <Stack gap="xl">
            <Stack gap={4}>
                <Title order={2} fw={700} style={{ letterSpacing: '-0.5px' }}>
                    設定
                </Title>
                <Text c="dimmed">アプリケーションのデフォルト設定を管理します。</Text>
            </Stack>

            <Stack gap="lg">
                {/* Output Directory */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        出力先設定
                    </Text>
                    <Group align="flex-end">
                        <TextInput
                            label="出力ルートディレクトリ"
                            description="OCR結果（resultフォルダ）の保存先を変更する場合に指定します（空の場合はプロジェクト標準のresultが使用されます）"
                            placeholder="デフォルト (Project Root/result)"
                            value={settings.outputRoot || ''}
                            onChange={(e) => setSettings({ ...settings, outputRoot: e.target.value })}
                            flex={1}
                        />
                        <Button variant="light" leftSection={<IconFolder size={16} />} onClick={handleBrowse}>
                            参照
                        </Button>
                        <Button
                            variant="subtle"
                            color="gray"
                            onClick={() => setSettings({ ...settings, outputRoot: undefined })}
                            disabled={!settings.outputRoot}
                        >
                            クリア
                        </Button>
                    </Group>
                </Card>

                {/* Window Settings */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        ウィンドウ設定
                    </Text>
                    <Text size="xs" c="dimmed" mb="md">
                        ウィンドウサイズは次回起動時に反映されます。
                    </Text>
                    <Group justify="flex-end" mb="xs">
                        <Button
                            variant="light"
                            size="xs"
                            onClick={async () => {
                                try {
                                    const { width, height } = await getCurrentWindowSize()
                                    setSettings((prev) => (prev ? {
                                        ...prev,
                                        windowWidth: Math.max(720, width),
                                        windowHeight: Math.max(540, height),
                                    } : prev))
                                } catch (err) {
                                    console.error(err)
                                    notifications.show({
                                        title: 'Error',
                                        message: '現在のウィンドウサイズを取得できませんでした',
                                        color: 'red',
                                    })
                                }
                            }}
                        >
                            現在のサイズで設定
                        </Button>
                    </Group>
                    <Group grow>
                        <NumberInput
                            label="幅 (px)"
                            min={720}
                            max={3840}
                            value={settings.windowWidth ?? 1200}
                            onChange={(v) => {
                                const parsed = typeof v === 'number' ? v : null
                                if (!parsed) return
                                setSettings((prev) => (prev ? { ...prev, windowWidth: parsed } : prev))
                            }}
                        />
                        <NumberInput
                            label="高さ (px)"
                            min={540}
                            max={2160}
                            value={settings.windowHeight ?? 760}
                            onChange={(v) => {
                                const parsed = typeof v === 'number' ? v : null
                                if (!parsed) return
                                setSettings((prev) => (prev ? { ...prev, windowHeight: parsed } : prev))
                            }}
                        />
                    </Group>
                </Card>

                {/* Default Formats */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        デフォルト出力形式
                    </Text>
                    <Checkbox.Group
                        value={settings.formats}
                        onChange={(vals) => setSettings({ ...settings, formats: vals })}
                    >
                        <Group mt="xs">
                            <Checkbox value="md" label="Markdown" />
                            <Checkbox value="docx" label="Word (docx)" />
                            <Checkbox value="xlsx" label="Excel (xlsx)" />
                            <Checkbox value="csv" label="CSV" />
                        </Group>
                    </Checkbox.Group>
                </Card>

                {/* Processing Options */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        処理オプション (デフォルト)
                    </Text>
                    <Stack gap="md">
                        <Switch
                            label="画像をPDF化してから処理"
                            description="複数の画像を1つのPDFとしてまとめて処理します"
                            checked={settings.imageAsPdf}
                            onChange={() =>
                                setSettings((prev) => (prev ? { ...prev, imageAsPdf: !prev.imageAsPdf } : prev))
                            }
                        />
                        <Divider />
                        <Switch
                            label="図表抽出 (Experimental)"
                            description="図表を画像として切り出します"
                            checked={settings.enableFigure}
                            onChange={() =>
                                setSettings((prev) => (prev ? { ...prev, enableFigure: !prev.enableFigure } : prev))
                            }
                        />
                    </Stack>
                </Card>

                {/* Performance Settings */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        パフォーマンス
                    </Text>
                    <Switch
                        label="GPU で処理する（対応環境のみ）"
                        description="ON にすると GPU を優先して利用します。動作が不安定な場合は OFF を推奨します。"
                        checked={Boolean(settings.useGpu)}
                        onChange={() =>
                            setSettings((prev) => (prev ? { ...prev, useGpu: !prev.useGpu } : prev))
                        }
                    />
                </Card>

                {/* Stability / Expert Settings */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        安定運用設定 (PDF)
                    </Text>
                    <Stack gap="md">
                        <Stack gap={6}>
                            <Text size="sm" fw={500}>PDF→画像変換 DPI</Text>
                            <Text size="xs" c="dimmed">
                                DPI を上げるほど細部の認識精度は上がりますが、処理時間/メモリ使用量が増えます
                            </Text>
                            <SegmentedControl
                                value={dpiPreset}
                                onChange={(v) => {
                                    setDpiPreset(v as typeof dpiPreset)
                                    if (v === 'custom') return
                                    const parsed = Number(v)
                                    if (!Number.isFinite(parsed)) return
                                    setSettings((prev) => (prev ? { ...prev, pdfDpi: parsed } : prev))
                                }}
                                data={[
                                    { label: '低 (200)', value: '200' },
                                    { label: '標準 (300)', value: '300' },
                                    { label: '高精細 (400)', value: '400' },
                                    { label: 'カスタム', value: 'custom' },
                                ]}
                            />
                            {dpiPreset === 'custom' && (
                                <NumberInput
                                    label="カスタム DPI"
                                    min={72}
                                    max={600}
                                    value={dpiValue}
                                    onChange={(v) => {
                                        const parsed = typeof v === 'number' ? v : null
                                        if (!parsed) return
                                        setSettings((prev) => (prev ? { ...prev, pdfDpi: parsed } : prev))
                                    }}
                                />
                            )}
                        </Stack>
                        <Divider />
                        <NumberInput
                            label="チャンクサイズ"
                            description="1回に処理するページ数（メモリ不足対策）"
                            min={1}
                            max={100}
                            value={settings.chunkSize ?? 10}
                            onChange={(v) =>
                                setSettings((prev) => (prev ? { ...prev, chunkSize: Number(v) } : prev))
                            }
                        />
                        <Divider />
                        <Switch
                            label="休憩を有効にする"
                            description="Chunk処理ごとに待機時間を挟みます（CPU/API負荷軽減）"
                            checked={settings.enableRest}
                            onChange={() =>
                                setSettings((prev) => (prev ? { ...prev, enableRest: !prev.enableRest } : prev))
                            }
                        />
                        <Collapse in={settings.enableRest}>
                            <NumberInput
                                mt="md"
                                label="休憩時間 (秒)"
                                min={1}
                                max={300}
                                value={settings.restSeconds ?? 10}
                                onChange={(v) =>
                                    setSettings((prev) => (prev ? { ...prev, restSeconds: Number(v) } : prev))
                                }
                            />
                        </Collapse>
                    </Stack>
                </Card>

                <Group justify="flex-end">
                    <Button
                        size="lg"
                        leftSection={<IconDeviceFloppy size={20} />}
                        loading={loading}
                        onClick={handleSave}
                    >
                        設定を保存
                    </Button>
                </Group>
            </Stack>
        </Stack>
    )
})
