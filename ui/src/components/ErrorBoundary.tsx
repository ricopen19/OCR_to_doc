import { Component, type ReactNode } from 'react'
import { Button, Card, Container, Stack, Text, Title } from '@mantine/core'

type Props = {
  children: ReactNode
}

type State = {
  error: Error | null
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  componentDidCatch(error: Error, info: unknown) {
    console.error('[ErrorBoundary]', error, info)
  }

  render() {
    if (!this.state.error) return this.props.children

    return (
      <Container size="sm" py="xl">
        <Card withBorder shadow="sm" radius="lg" padding="lg">
          <Stack gap="md">
            <Title order={3}>画面の描画でエラーが発生しました</Title>
            <Text size="sm" c="dimmed">
              まずは再読み込みをお試しください。継続する場合は、開発者ツールのコンソールログも確認してください。
            </Text>
            <Text size="sm" style={{ fontFamily: 'monospace', whiteSpace: 'pre-wrap' }}>
              {String(this.state.error?.stack || this.state.error?.message || this.state.error)}
            </Text>
            <Button onClick={() => window.location.reload()} color="blue">
              再読み込み
            </Button>
          </Stack>
        </Card>
      </Container>
    )
  }
}

