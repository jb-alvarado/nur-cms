export type PluginAdminContext = {
    pluginId: string
    roles: () => readonly string[]
    hasRole: (role: string) => boolean
    locale: () => string
    theme: () => 'light' | 'dark'
    request: (path: string, init?: RequestInit) => Promise<Response>
}
