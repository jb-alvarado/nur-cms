export type PluginAdminMenuItem = {
    label: string
    labels: Record<string, string>
    path: string
    icon?: string | null
    access?: string | null
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

export type PluginAdminLocation = {
    path: string
    relativePath: string
    search: string
    hash: string
}

export type PluginAdminTheme = 'light' | 'dark'

export type PluginAdminContext = {
    pluginId: string
    roles: () => readonly string[]
    hasRole: (role: string) => boolean
    locale: () => string
    theme: () => PluginAdminTheme
    location: () => PluginAdminLocation
    onLocationChange: (listener: (location: PluginAdminLocation) => void) => () => void
    onLocaleChange: (listener: (locale: string) => void) => () => void
    onThemeChange: (listener: (theme: PluginAdminTheme) => void) => () => void
    request: (path: string, init?: RequestInit) => Promise<Response>
    navigate: (path?: string) => Promise<void>
    notify: (variance: 'info' | 'success' | 'warning' | 'error', text: string) => void
}

export function roleName(role: Role): string {
    return typeof role === 'string' ? role : role.custom
}

export function pluginAllowsRole(plugin: PluginMetadata, role: Role): boolean {
    return plugin.admin ? accessAllowsRole(plugin.admin.access, roleName(role)) : false
}

export function menuAllowsRole(item: PluginAdminMenuItem, admin: PluginAdmin, role: Role): boolean {
    return accessAllowsRole(item.access ?? admin.access, roleName(role))
}

export function pluginAllowsPath(plugin: PluginMetadata, role: Role, path: string): boolean {
    const admin = plugin.admin
    if (!admin || !pluginAllowsRole(plugin, role)) return false

    const namespace = `/admin/plugins/${plugin.id}`
    if (path === namespace || path === `${namespace}/`) return true

    return admin.menu.some((item) => {
        if (!menuAllowsRole(item, admin, role)) return false
        const rawPath = item.path.split(/[?#]/, 1)[0]
        const menuPath = rawPath.endsWith('/') ? rawPath.slice(0, -1) : rawPath
        return path === menuPath || path.startsWith(`${menuPath}/`)
    })
}

export function pluginMenuLabel(item: PluginAdminMenuItem, locale: string): string {
    return item.labels[locale] ?? item.labels[locale.split('-', 1)[0]] ?? item.label
}

function accessAllowsRole(access: string, role: string): boolean {
    return access
        .split(',')
        .map((value) => value.trim())
        .includes(role)
}
