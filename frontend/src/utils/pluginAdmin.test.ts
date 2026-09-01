import { describe, expect, it, vi } from 'vitest'

import {
    createSubscription,
    pluginAdminLocation,
    pluginViewKey,
    resolvePluginAdminNavigation,
} from './pluginAdmin'

describe('plugin admin navigation', () => {
    it('resolves relative routes, query-only navigation, and hashes', () => {
        expect(resolvePluginAdminNavigation('example', '/admin/plugins/example', 'products/12')).toBe(
            '/admin/plugins/example/products/12',
        )
        expect(resolvePluginAdminNavigation('example', '/admin/plugins/example/orders?page=1', '?page=2')).toBe(
            '/admin/plugins/example/orders?page=2',
        )
        expect(resolvePluginAdminNavigation('example', '/admin/plugins/example/orders', '#details')).toBe(
            '/admin/plugins/example/orders#details',
        )
    })

    it('rejects external and foreign admin paths', () => {
        expect(() => resolvePluginAdminNavigation('example', '/admin/plugins/example', 'https://example.org')).toThrow()
        expect(() => resolvePluginAdminNavigation('example', '/admin/plugins/example', '/admin/plugins/other')).toThrow()
        expect(() => resolvePluginAdminNavigation('example', '/admin/plugins/example', '../../configuration')).toThrow()
    })

    it('provides path components relative to the plugin namespace', () => {
        expect(pluginAdminLocation('example', '/admin/plugins/example/products/12?tab=stock#price')).toEqual({
            path: '/admin/plugins/example/products/12',
            relativePath: '/products/12',
            search: '?tab=stock',
            hash: '#price',
        })
    })

    it('keeps the component key stable within one plugin', () => {
        expect(pluginViewKey('example')).toBe(pluginViewKey('example'))
        expect(pluginViewKey('example')).not.toBe(pluginViewKey('other'))
    })
})

describe('plugin context subscriptions', () => {
    it('delivers forward and backward locations without recreating listeners', () => {
        const subscription = createSubscription(vi.fn())
        const listener = vi.fn()
        subscription.subscribe(listener)
        const products = pluginAdminLocation('example', '/admin/plugins/example/products')
        const orders = pluginAdminLocation('example', '/admin/plugins/example/orders?page=2')

        subscription.emit(products)
        subscription.emit(orders)
        subscription.emit(products)

        expect(listener).toHaveBeenNthCalledWith(1, products)
        expect(listener).toHaveBeenNthCalledWith(2, orders)
        expect(listener).toHaveBeenNthCalledWith(3, products)
    })

    it('unsubscribes individually and clears every listener on unmount', () => {
        const subscription = createSubscription(vi.fn())
        const first = vi.fn()
        const second = vi.fn()
        const unsubscribe = subscription.subscribe(first)
        subscription.subscribe(second)

        unsubscribe()
        subscription.emit('before clear')
        subscription.clear()
        subscription.emit('after clear')

        expect(first).not.toHaveBeenCalled()
        expect(second).toHaveBeenCalledOnce()
        expect(second).toHaveBeenCalledWith('before clear')
    })

    it('delivers locale and theme changes independently', () => {
        const locale = createSubscription<string>(vi.fn())
        const theme = createSubscription<'light' | 'dark'>(vi.fn())
        const localeListener = vi.fn()
        const themeListener = vi.fn()
        locale.subscribe(localeListener)
        theme.subscribe(themeListener)

        locale.emit('de')
        theme.emit('dark')

        expect(localeListener).toHaveBeenCalledWith('de')
        expect(themeListener).toHaveBeenCalledWith('dark')
    })

    it('isolates failing listeners', () => {
        const onError = vi.fn()
        const subscription = createSubscription<string>(onError)
        const healthy = vi.fn()
        subscription.subscribe(() => {
            throw new Error('listener failed')
        })
        subscription.subscribe(healthy)

        subscription.emit('dark')

        expect(onError).toHaveBeenCalledOnce()
        expect(healthy).toHaveBeenCalledWith('dark')
    })
})
