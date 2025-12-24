import { Stack, Text, NavLink } from '@mantine/core'
import {
    IconHome,
    IconPlayerPlay,
    IconSettings,
    IconFileDownload,
} from '@tabler/icons-react'

export type PageKey = 'home' | 'run' | 'settings' | 'result'

interface SidebarProps {
    activePage: PageKey
    setPage: (page: PageKey) => void
}

const navItems: { key: PageKey; label: string; icon: React.ReactNode }[] = [
    { key: 'home', label: 'ホーム', icon: <IconHome size={18} stroke={1.5} /> },
    { key: 'run', label: '実行', icon: <IconPlayerPlay size={18} stroke={1.5} /> },
    { key: 'result', label: '結果', icon: <IconFileDownload size={18} stroke={1.5} /> },
    { key: 'settings', label: '設定', icon: <IconSettings size={18} stroke={1.5} /> },
]

export function Sidebar({ activePage, setPage }: SidebarProps) {
    return (
        <Stack gap="xs">
            <Text 
                fw={600} 
                size="xs" 
                c="dimmed" 
                mb={4} 
                tt="uppercase" 
                style={{ letterSpacing: '0.5px' }}
            >
                Menu
            </Text>
            {navItems.map((item) => (
                <NavLink
                    key={item.key}
                    label={item.label}
                    leftSection={item.icon}
                    active={activePage === item.key}
                    onClick={() => setPage(item.key)}
                    variant="light"
                    color={activePage === item.key ? 'blue' : 'gray'}
                    styles={(theme) => ({
                        root: {
                            borderRadius: theme.radius.md,
                            fontWeight: 500,
                            padding: '10px 12px',
                            transition: 'all 0.2s ease',
                            maxWidth: '100%',
                            overflow: 'hidden',
                            '&:hover': {
                                backgroundColor: activePage === item.key
                                    ? theme.colors.blue[0]
                                    : theme.colors.gray[0],
                            },
                        },
                        label: {
                            fontSize: theme.fontSizes.sm,
                        },
                    })}
                />
            ))}
        </Stack>
    )
}
