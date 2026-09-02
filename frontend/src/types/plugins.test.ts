import { describe, expect, it } from 'vitest'

import {
    menuAllowsRole,
    pluginAllowsPath,
    pluginMenuLabel,
    type PluginAdminMenuItem,
    type PluginMetadata,
} from './plugins'

const menu: PluginAdminMenuItem[] = [
    {
        label: 'Statistics',
        labels: { de: 'Statistiken', en: 'Statistics' },
        path: '/admin/plugins/example/statistics',
        access: 'admin,stat',
    },
    {
        label: 'Products',
        labels: { de: 'Produkte' },
        path: '/admin/plugins/example/products',
        access: 'admin',
    },
    {
        label: 'Shared',
        labels: {},
        path: '/admin/plugins/example/shared',
    },
]

const plugin: PluginMetadata = {
    id: 'example',
    name: 'Example',
    version: '1.0.0',
    admin: {
        entry: 'admin.js',
        element: 'example-admin',
        access: 'admin,stat',
        styles: [],
        menu,
    },
}

describe('plugin admin permissions', () => {
    it('inherits admin access when a menu item has no access declaration', () => {
        expect(menuAllowsRole(menu[2], plugin.admin!, { custom: 'stat' })).toBe(true)
    })

    it('rejects direct paths outside the visible role-specific menu', () => {
        expect(pluginAllowsPath(plugin, { custom: 'stat' }, '/admin/plugins/example/statistics/2026')).toBe(true)
        expect(pluginAllowsPath(plugin, { custom: 'stat' }, '/admin/plugins/example/products')).toBe(false)
        expect(pluginAllowsPath(plugin, { custom: 'stat' }, '/admin/plugins/example/product')).toBe(false)
    })

    it('accepts details below an accessible menu path with a trailing slash', () => {
        const trailing = structuredClone(plugin)
        trailing.admin!.menu[0].path = '/admin/plugins/example/statistics/'

        expect(pluginAllowsPath(trailing, { custom: 'stat' }, '/admin/plugins/example/statistics/2026')).toBe(true)
    })
})

describe('plugin menu translations', () => {
    it('uses the active locale and falls back to the default label', () => {
        expect(pluginMenuLabel(menu[0], 'de')).toBe('Statistiken')
        expect(pluginMenuLabel(menu[0], 'de-AT')).toBe('Statistiken')
        expect(pluginMenuLabel(menu[1], 'fr')).toBe('Products')
    })
})
