
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useState } from 'react'
import {
    Stack,
    Title,
    Text,
    Card,
    Button,
    Group,
    TextInput,
    Switch,
    Divider,
    NumberInput,
    Collapse,
    SegmentedControl,
    Select,
    PasswordInput,
} from '@mantine/core'
import { IconDeviceFloppy, IconFolder } from '@tabler/icons-react'
import { open } from '@tauri-apps/plugin-dialog'
import { notifications } from '@mantine/notifications'
import { getCurrentWindowSize, type AppSettings, loadSettings, saveSettings } from '../api/settings'
import { listOcrModels } from '../api/runJob'

export type SettingsHandle = {
    isDirty: () => boolean
    save: () => Promise<boolean>
}

type SettingsProps = {
    onSaved?: (settings: AppSettings) => void
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

export const Settings = forwardRef<SettingsHandle, SettingsProps>(function Settings(props, ref) {
    const [settings, setSettings] = useState<AppSettings | null>(null)
    const [initialSnapshot, setInitialSnapshot] = useState<string | null>(null)
    const [loading, setLoading] = useState(false)
    const [fetchedModels, setFetchedModels] = useState<string[]>([])
    const [modelsLoading, setModelsLoading] = useState(false)
    const [modelError, setModelError] = useState<string | null>(null)

    const modelOptions = useMemo(() => {
        const set = new Set<string>(fetchedModels)
        const cur = settings?.ocrModel?.trim()
        if (cur) set.add(cur)
        return Array.from(set)
    }, [fetchedModels, settings?.ocrModel])

    const fetchModels = useCallback(async () => {
        if (!settings) return
        setModelsLoading(true)
        setModelError(null)
        try {
            const list = await listOcrModels(
                settings.ocrEngine ?? 'ollama',
                settings.llamaBaseUrl?.trim() || undefined,
                settings.llamaApiKey?.trim() || undefined,
            )
            setFetchedModels(list)
            if (list.length === 0) setModelError('モデルが見つかりませんでした')
        } catch (e) {
            setModelError(String(e))
        } finally {
            setModelsLoading(false)
        }
    }, [settings])

    useEffect(() => {
        loadSettings().then((s) => {
            const normalized: AppSettings = {
                ...s,
                previewQuality: s.previewQuality ?? 'light',
            }
            setSettings(normalized)
            setInitialSnapshot(settingsSnapshot(normalized))
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
                previewQuality: settings.previewQuality ?? 'light',
            }
            await saveSettings(toSave)
            setSettings(toSave)
            setInitialSnapshot(settingsSnapshot(toSave))
            props.onSaved?.(toSave)
            notifications.show({
                title: '保存しました',
                message: '設定を保存しました',
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
                            description="OCR結果（resultフォルダ）の保存先を変更する場合に指定します（空の場合は既定の出力先を使用します）"
                            placeholder="デフォルト (~/Library/Application Support/ocr-to-doc/result)"
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

                {/* Note */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        実行画面で保存する項目
                    </Text>
                    <Text size="sm" c="dimmed">
                        出力形式 / 表出力モード / 処理モード / PDF DPI のデフォルトは「実行」画面で設定し、
                        「デフォルトに保存」から保存してください。
                    </Text>
                </Card>

                {/* OCR Engine */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        OCR エンジン
                    </Text>
                    <Stack gap="md">
                        <Stack gap={4}>
                            <Text size="sm" fw={600}>エンジン</Text>
                            <SegmentedControl
                                data={[
                                    { label: 'Ollama（推奨）', value: 'ollama' },
                                    { label: 'llama.cpp', value: 'llamacpp' },
                                ]}
                                value={settings.ocrEngine ?? 'ollama'}
                                onChange={(v) =>
                                    setSettings((prev) =>
                                        prev ? { ...prev, ocrEngine: v as 'ollama' | 'llamacpp' } : prev
                                    )
                                }
                            />
                            <Text size="xs" c="dimmed">
                                llama.cpp はこの PC で起動している llama-server に接続します（上級者向け）。
                                サーバーの起動・モデルの用意は各自で行ってください。
                            </Text>
                        </Stack>

                        {settings.ocrEngine === 'llamacpp' && (
                            <>
                                <TextInput
                                    label="llama-server の接続先"
                                    description="空欄で既定（http://localhost:8080）"
                                    placeholder="http://localhost:8080"
                                    value={settings.llamaBaseUrl ?? ''}
                                    onChange={(e) =>
                                        setSettings((prev) =>
                                            prev ? { ...prev, llamaBaseUrl: e.target.value || undefined } : prev
                                        )
                                    }
                                />
                                <PasswordInput
                                    label="API キー"
                                    description="llama-server が認証を要求する場合のみ"
                                    value={settings.llamaApiKey ?? ''}
                                    onChange={(e) =>
                                        setSettings((prev) =>
                                            prev ? { ...prev, llamaApiKey: e.target.value || undefined } : prev
                                        )
                                    }
                                />
                            </>
                        )}

                        <Group align="flex-end">
                            <Select
                                label="OCR モデル"
                                description="「再取得」で接続先から一覧を読み込みます"
                                placeholder="glm-ocr"
                                data={modelOptions}
                                value={settings.ocrModel ?? 'glm-ocr'}
                                onChange={(v) =>
                                    setSettings((prev) => (prev ? { ...prev, ocrModel: v ?? 'glm-ocr' } : prev))
                                }
                                searchable
                                flex={1}
                            />
                            <Button variant="light" loading={modelsLoading} onClick={fetchModels}>
                                再取得
                            </Button>
                        </Group>
                        {modelError && (
                            <Text size="xs" c="red">{modelError}</Text>
                        )}
                    </Stack>
                </Card>

                {/* Excel Settings */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        Excel
                    </Text>
                    <Switch
                        label="Excelのメタシートを付与"
                        description="xlsx出力時にシート一覧や変換条件などの情報を追加します"
                        checked={settings.excelMetaSheet ?? true}
                        onChange={() =>
                            setSettings((prev) =>
                                prev ? { ...prev, excelMetaSheet: !(prev.excelMetaSheet ?? true) } : prev
                            )
                        }
                    />
                </Card>

                {/* Processing Options */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        処理オプション (デフォルト)
                    </Text>
                    <Stack gap="md">
                        <Switch
                            label="図表抽出 (Experimental)"
                            description="図表を画像として切り出します"
                            checked={settings.enableFigure}
                            onChange={() =>
                                setSettings((prev) => (prev ? { ...prev, enableFigure: !prev.enableFigure } : prev))
                            }
                        />
                        <Divider />
                        <Switch
                            label="表を高精度で再OCR（glm-ocr 使用）"
                            description="表の行・列構造を復元します。メモリ 8GB 以下の PC では OFF 推奨"
                            checked={settings.enableTableReocr}
                            onChange={() =>
                                setSettings((prev) =>
                                    prev ? { ...prev, enableTableReocr: !prev.enableTableReocr } : prev
                                )
                            }
                        />
                    </Stack>
                </Card>

                {/* Performance Settings */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        パフォーマンス
                    </Text>
                    <Stack gap="md">
                        <Stack gap={4}>
                            <Text size="sm" fw={600}>
                                トリミングプレビュー品質
                            </Text>
                            <Text size="xs" c="dimmed">
                                プレビュー時の解像度を調整します（最終出力は影響しません）。
                            </Text>
                            <SegmentedControl
                                value={settings.previewQuality ?? 'light'}
                                onChange={(value) =>
                                    setSettings((prev) =>
                                        prev
                                            ? {
                                                  ...prev,
                                                  previewQuality: value as AppSettings['previewQuality'],
                                              }
                                            : prev
                                    )
                                }
                                data={[
                                    { label: '軽量', value: 'light' },
                                    { label: '標準', value: 'standard' },
                                    { label: '高品質', value: 'high' },
                                ]}
                            />
                        </Stack>
                    </Stack>
                </Card>

                {/* Stability / Expert Settings */}
                <Card withBorder shadow="sm" radius="lg" padding="lg">
                    <Text fw={600} size="sm" c="dimmed" tt="uppercase" mb="sm" style={{ letterSpacing: '0.5px' }}>
                        安定運用設定
                    </Text>
                    <Stack gap="md">
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
