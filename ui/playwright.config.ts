import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests',
  use: {
    headless: true,
    viewport: { width: 1280, height: 720 },
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1 --port 5173',
    url: 'http://127.0.0.1:5173',
    timeout: 120 * 1000,
    reuseExistingServer: !process.env.CI,
  },
})
