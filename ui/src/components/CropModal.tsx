import {
  Alert,
  Button,
  Card,
  Group,
  Modal,
  NumberInput,
  Stack,
  Tabs,
  Text,
} from '@mantine/core'
import { IconAlertTriangle, IconCheck, IconTrash } from '@tabler/icons-react'
import { useEffect, useMemo, useState } from 'react'
import { notifications } from '@mantine/notifications'
import { renderPreview } from '../api/preview'
import { ImageCropper } from './ImageCropper'
import type { CropRect } from '../types/crop'

type CropModalProps = {
  opened: boolean
  onClose: () => void
  filePath: string
  initialCrop?: CropRect
  onSave: (crop?: CropRect) => void
}

function isPdf(path: string) {
  return path.toLowerCase().endsWith('.pdf')
}

export function CropModal({ opened, onClose, filePath, initialCrop, onSave }: CropModalProps) {
  const pdf = useMemo(() => isPdf(filePath), [filePath])
  const [tab, setTab] = useState<string>('select')

  const [pageCount, setPageCount] = useState<number | null>(null)
  const [basePage, setBasePage] = useState<number>(1)
  const [previewPage, setPreviewPage] = useState<number>(1)

  const [selectSrc, setSelectSrc] = useState<string | null>(null)
  const [appliedSrc, setAppliedSrc] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [draftCrop, setDraftCrop] = useState<CropRect | undefined>(initialCrop)
  const hadInitialCrop = useMemo(() => Boolean(initialCrop), [initialCrop])

  const loadSelectPreview = async (page: number) => {
    setLoading(true)
    setError(null)
    try {
      const res = await renderPreview(filePath, { page, maxLongEdge: 1600 })
      setSelectSrc(res.dataUrl)
      setPageCount(res.pageCount ?? null)
      setBasePage(res.page ?? page)
    } catch (e) {
      setError(String(e))
      setSelectSrc(null)
    } finally {
      setLoading(false)
    }
  }

  const loadAppliedPreview = async (page: number, crop: CropRect) => {
    setLoading(true)
    setError(null)
    try {
      const res = await renderPreview(filePath, { page, crop, maxLongEdge: 1600 })
      setAppliedSrc(res.dataUrl)
      setPageCount(res.pageCount ?? null)
      setPreviewPage(res.page ?? page)
    } catch (e) {
      setError(String(e))
      setAppliedSrc(null)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (!opened) return
    setTab('select')
    setError(null)
    setDraftCrop(initialCrop)
    setSelectSrc(null)
    setAppliedSrc(null)
    setPageCount(null)
    setBasePage(1)
    setPreviewPage(1)
    void loadSelectPreview(1)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opened, filePath])

  useEffect(() => {
    if (!opened) return
    if (!pdf) return
    void loadSelectPreview(basePage)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [basePage])

  useEffect(() => {
    if (!opened) return
    if (tab !== 'preview') return
    if (!draftCrop) return
    void loadAppliedPreview(previewPage, draftCrop)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, previewPage, draftCrop])

  const cropLabel = useMemo(() => {
    if (!draftCrop) return '未設定'
    const l = Math.round(draftCrop.left * 1000) / 10
    const t = Math.round(draftCrop.top * 1000) / 10
    const w = Math.round(draftCrop.width * 1000) / 10
    const h = Math.round(draftCrop.height * 1000) / 10
    return `${l}%, ${t}%, ${w}%, ${h}%`
  }, [draftCrop])

  return (
    <Modal
      opened={opened}
      onClose={onClose}
      title="トリミング"
      size="xl"
      radius="lg"
      centered
      overlayProps={{ blur: 1 }}
    >
      <Stack gap="md">
        <Text size="sm" c="dimmed" style={{ wordBreak: 'break-all' }}>
          {filePath}
        </Text>

        {error && (
          <Alert color="red" icon={<IconAlertTriangle size={16} />} title="プレビュー取得に失敗しました">
            {error}
          </Alert>
        )}

        <Tabs value={tab} onChange={(v) => setTab(v || 'select')}>
          <Tabs.List>
            <Tabs.Tab value="select">範囲指定</Tabs.Tab>
            <Tabs.Tab value="preview" disabled={!draftCrop}>
              適用後プレビュー
            </Tabs.Tab>
          </Tabs.List>

          <Tabs.Panel value="select" pt="md">
            <Stack gap="md">
              {pdf && (
                <Group align="end">
                  <NumberInput
                    label="基準ページ"
                    min={1}
                    max={pageCount ?? undefined}
                    value={basePage}
                    onChange={(v) => setBasePage(typeof v === 'number' ? v : 1)}
                    w={180}
                  />
                  <Text size="sm" c="dimmed">
                    {pageCount ? `/${pageCount}` : ''}
                  </Text>
                  <Button
                    variant="light"
                    onClick={() => void loadSelectPreview(basePage)}
                    loading={loading}
                  >
                    再読み込み
                  </Button>
                </Group>
              )}

              <Card withBorder radius="md" padding="md">
                <Stack gap="xs">
                  <Group justify="space-between">
                    <Text size="sm" fw={600}>
                      選択範囲（left, top, width, height）
                    </Text>
                    <Text size="sm" c="dimmed">
                      {cropLabel}
                    </Text>
                  </Group>

                  {selectSrc ? (
                    <ImageCropper
                      src={selectSrc}
                      value={draftCrop}
                      onChange={(next) => setDraftCrop(next)}
                    />
                  ) : (
                    <Text size="sm" c="dimmed">
                      プレビューを表示できません（Tauri 環境で実行してください）
                    </Text>
                  )}

                  <Group justify="flex-end" mt="xs">
                    <Button
                      variant="subtle"
                      color="red"
                      leftSection={<IconTrash size={16} />}
                      onClick={() => setDraftCrop(undefined)}
                    >
                      解除
                    </Button>
                    <Button
                      variant="light"
                      leftSection={<IconCheck size={16} />}
                      disabled={!draftCrop}
                      onClick={() => {
                        setTab('preview')
                        setPreviewPage(basePage)
                      }}
                    >
                      適用後を確認
                    </Button>
                  </Group>
                </Stack>
              </Card>
            </Stack>
          </Tabs.Panel>

          <Tabs.Panel value="preview" pt="md">
            {!draftCrop ? (
              <Text size="sm" c="dimmed">
                範囲が未設定です。
              </Text>
            ) : (
              <Stack gap="md">
                {pdf ? (
                  <Group align="end">
                    <Button
                      variant="light"
                      disabled={loading || previewPage <= 1}
                      onClick={() => setPreviewPage((p) => Math.max(1, p - 1))}
                    >
                      前へ
                    </Button>
                    <NumberInput
                      label="ページ"
                      min={1}
                      max={pageCount ?? undefined}
                      value={previewPage}
                      onChange={(v) => setPreviewPage(typeof v === 'number' ? v : 1)}
                      w={180}
                    />
                    <Text size="sm" c="dimmed">
                      {pageCount ? `/${pageCount}` : ''}
                    </Text>
                    <Button
                      variant="light"
                      disabled={loading || (pageCount != null && previewPage >= pageCount)}
                      onClick={() =>
                        setPreviewPage((p) => (pageCount ? Math.min(pageCount, p + 1) : p + 1))
                      }
                    >
                      次へ
                    </Button>
                  </Group>
                ) : (
                  <Text size="sm" c="dimmed">
                    画像は1枚のみです
                  </Text>
                )}

                <Card withBorder radius="md" padding="md">
                  {appliedSrc ? (
                    <img
                      src={appliedSrc}
                      alt="cropped preview"
                      style={{
                        width: '100%',
                        height: 'auto',
                        display: 'block',
                        borderRadius: 'var(--mantine-radius-md)',
                        border: '1px solid var(--mantine-color-gray-3)',
                      }}
                    />
                  ) : (
                    <Text size="sm" c="dimmed">
                      プレビューを読み込み中…
                    </Text>
                  )}
                </Card>
              </Stack>
            )}
          </Tabs.Panel>
        </Tabs>

        <Group justify="space-between" mt="xs">
          <Button variant="default" onClick={onClose}>
            キャンセル
          </Button>
          <Button
            onClick={() => {
              onSave(draftCrop)
              notifications.show({
                title: '保存しました',
                message: draftCrop ? 'トリミング設定を反映しました' : 'トリミング設定を解除しました',
                color: draftCrop ? 'green' : 'blue',
              })
              onClose()
            }}
          >
            {draftCrop ? '保存' : hadInitialCrop ? '解除して保存' : '保存'}
          </Button>
        </Group>
      </Stack>
    </Modal>
  )
}
