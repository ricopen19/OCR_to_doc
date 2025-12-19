import { test, expect } from '@playwright/test'
import path from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const samplePath = path.resolve(__dirname, '../../sample.pdf')

test('D&Dでファイル選択後、モック変換が完了する', async ({ page }) => {
  await page.goto('http://127.0.0.1:5173/')
  await page.locator('nav').getByText('実行', { exact: true }).click()

  // Dropzone の hidden input に直接ファイルを設定
  const dropInput = page.locator('[data-testid="dropzone"] input[type="file"]')
  await dropInput.setInputFiles(samplePath)

  await expect(page.getByRole('listitem').filter({ hasText: 'sample.pdf' }).first()).toBeVisible()

  await page.getByRole('button', { name: '実行' }).click()

  await expect(page.getByText('done')).toBeVisible()

  await page.locator('nav').getByText('結果', { exact: true }).click()
  await expect(page.getByText('Converted markdown for: sample.pdf')).toBeVisible()
})
