export type PluginAdminMenuItem = {
    label: string
    path: string
    icon?: string | null
}

export type PluginAdmin = {
    entry?: string | null
    element?: string | null
    access: string
    styles: string[]
    menu: PluginAdminMenuItem[]
}

export type PluginMetadata = {
    id: string
    version: string
    admin?: PluginAdmin | null
}

export type PluginAdminContext = {
    pluginId: string
    locale: () => string
    request: (path: string, init?: RequestInit) => Promise<Response>
    navigate: (path?: string) => Promise<void>
    notify: (variance: 'info' | 'success' | 'warning' | 'error', text: string) => void
}

export function roleName(role: Role): string {
    return typeof role === 'string' ? role : role.custom
}

export function pluginAllowsRole(plugin: PluginMetadata, role: Role): boolean {
    const current = roleName(role)
    return (
        plugin.admin?.access
            .split(',')
            .map((value) => value.trim())
            .includes(current) ?? false
    )
}
