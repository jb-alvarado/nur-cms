import type { PluginAdminLocation } from '@/types/plugins'

const ADMIN_PREFIX = '/admin/p'
const URL_ORIGIN = 'https://nur-cms.invalid'

export type Subscription<T> = {
    emit: (value: T) => void
    subscribe: (listener: (value: T) => void) => () => void
    clear: () => void
}

export function pluginAdminPath(pluginId: string): string {
    return `${ADMIN_PREFIX}/${encodeURIComponent(pluginId)}`
}

export function pluginViewKey(pluginId: string): string {
    return `plugin:${pluginId}`
}

export function resolvePluginApiRequest(pluginId: string, path: string): string {
    const namespace = `/api/p/${encodeURIComponent(pluginId)}`
    const absolute = path === namespace || path.startsWith(`${namespace}/`) || path.startsWith('/api/p/')
    const relative = path.startsWith('/') && !path.startsWith('//')
    const target = absolute ? path : relative ? `${namespace}${path === '/' ? '' : path}` : ''
    const url = new URL(target, URL_ORIGIN)

    if (
        !target ||
        url.origin !== URL_ORIGIN ||
        url.username ||
        url.password ||
        url.hash ||
        (url.pathname !== namespace && !url.pathname.startsWith(`${namespace}/`))
    ) {
        throw new Error('plugin request must stay inside its API namespace')
    }
    return `${url.pathname}${url.search}`
}

export function pluginAdminLocation(pluginId: string, fullPath: string): PluginAdminLocation {
    const namespace = pluginAdminPath(pluginId)
    const url = new URL(fullPath, URL_ORIGIN)
    const suffix = url.pathname.slice(namespace.length)

    return {
        path: url.pathname,
        relativePath: suffix.startsWith('/') ? suffix : '/',
        search: url.search,
        hash: url.hash,
    }
}

export function resolvePluginAdminNavigation(pluginId: string, currentFullPath: string, path = ''): string {
    const namespace = pluginAdminPath(pluginId)
    const root = new URL(`${namespace}/`, URL_ORIGIN)
    const current = new URL(currentFullPath, URL_ORIGIN)
    const base = path.startsWith('?') || path.startsWith('#') ? current : root
    const resolvedPath =
        path.startsWith(namespace) || path.startsWith('/admin/')
            ? path
            : path.startsWith('/')
              ? `${namespace}${path}`
              : path
    const target = resolvedPath ? new URL(resolvedPath, base) : root

    if (
        target.origin !== URL_ORIGIN ||
        target.username ||
        target.password ||
        (target.pathname !== namespace && !target.pathname.startsWith(`${namespace}/`))
    ) {
        throw new Error('plugin navigation must stay inside its admin namespace')
    }
    return `${target.pathname}${target.search}${target.hash}`
}

export function createSubscription<T>(onError: (error: unknown) => void): Subscription<T> {
    const listeners = new Set<(value: T) => void>()

    return {
        subscribe(listener) {
            if (typeof listener !== 'function') return () => undefined
            listeners.add(listener)
            return () => listeners.delete(listener)
        },
        emit(value) {
            for (const listener of [...listeners]) {
                try {
                    listener(value)
                } catch (error) {
                    onError(error)
                }
            }
        },
        clear() {
            listeners.clear()
        },
    }
}
