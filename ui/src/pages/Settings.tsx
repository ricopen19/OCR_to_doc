
import { useEffect, useState } from 'react'
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
} from '@mantine/core'
import { IconDeviceFloppy, IconFolder } from '@tabler/icons-react'
import { open } from '@tauri-apps/plugin-dialog'
import { notifications } from '@mantine/notifications'
import { type AppSettings, loadSettings, saveSettings } from '../api/settings'

export function Settings() {
    const [settings, setSettings] = useState<AppSettings | null>(null)
    const [loading, setLoading] = useState(false)

    useEffect(() => {
        loadSettings().then(setSettings).catch((err) => {
            console.error(err)
            notifications.show({
                title: 'Error',
                message: 'Failed to load settings',
                color: 'red',
            })
        })
    }, [])

    const handleSave = async () => {
        if (!settings) return
        setLoading(true)
        try {
            await saveSettings(settings)
            notifications.show({
                title: 'Success',
                message: 'Settings saved successfully',
                color: 'green',
            })
        } catch (err) {
            console.error(err)
            notifications.show({
                title: 'Error',
                message: 'Failed to save settings: ' + String(err),
                color: 'red',
            })
        } finally {
            setLoading(false)
        }
    }

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

    if (!settings) return <Text>Loading settings...</Text>

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
}
