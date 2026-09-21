export type PluginAdminContext = {
    pluginId: string
    roles: () => readonly string[]
    hasRole: (role: string) => boolean
    locale: () => string
    theme: () => 'light' | 'dark'
    request: (path: string, init?: RequestInit) => Promise<Response>
    selectMedia: (options?: { types?: string[] }) => Promise<{
        id: number | null
        url: string
        filename: string
        mimeType: string | null
        alt: string | null
        width: number | null
        height: number | null
    } | null>
}
