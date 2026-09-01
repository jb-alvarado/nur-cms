<script lang="ts">
const modules = new Map<string, Promise<void>>()
const styles = new Map<string, HTMLLinkElement>()
</script>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

import { authFetchRaw } from '@/composables/authFetch'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import {
    pluginAllowsPath,
    roleName,
    type PluginAdminContext,
    type PluginAdminLocation,
    type PluginAdminTheme,
    type PluginMetadata,
} from '@/types/plugins'
import {
    createSubscription,
    pluginAdminLocation,
    resolvePluginAdminNavigation,
    type Subscription,
} from '@/utils/pluginAdmin'

type PluginElement = HTMLElement & { context?: PluginAdminContext }

const route = useRoute()
const router = useRouter()
const auth = useAuth()
const store = useIndex()
const { t } = useI18n()
const container = ref<HTMLElement>()
const error = ref<string>()
const loading = ref(true)
let mountRevision = 0
let mountedPluginId: string | undefined
let subscriptions: PluginSubscriptions | undefined

type PluginSubscriptions = {
    location: Subscription<PluginAdminLocation>
    locale: Subscription<string>
    theme: Subscription<PluginAdminTheme>
}

function pluginFromRoute(): PluginMetadata | undefined {
    const id = typeof route.params.pluginId === 'string' ? route.params.pluginId : ''
    return store.plugins.find(
        (plugin) =>
            plugin.id === id &&
            plugin.admin?.entry &&
            plugin.admin.element &&
            pluginAllowsPath(plugin, auth.role, route.path),
    )
}

function assetUrl(plugin: PluginMetadata, asset: string): string {
    const encodedAsset = asset.split('/').map(encodeURIComponent).join('/')
    return `/plugins/${encodeURIComponent(plugin.id)}/assets/${encodedAsset}`
}

function loadPluginStyle(url: string) {
    if (styles.has(url)) return

    const link = document.createElement('link')
    link.rel = 'stylesheet'
    link.href = url
    link.addEventListener(
        'error',
        () => {
            link.remove()
            styles.delete(url)
            store.msgAlert('error', t('plugin.stylesheetLoadFailed', { url }))
        },
        { once: true },
    )
    styles.set(url, link)
    document.head.append(link)
}

function loadPluginModule(url: string): Promise<void> {
    const existing = modules.get(url)
    if (existing) return existing

    const loadingModule = import(/* @vite-ignore */ url)
        .then(() => undefined)
        .catch((error: unknown) => {
            modules.delete(url)
            throw error
        })
    modules.set(url, loadingModule)
    return loadingModule
}

function contextFor(plugin: PluginMetadata, listeners: PluginSubscriptions): PluginAdminContext {
    const prefix = `/api/plugins/${plugin.id}`
    const roles = () => Object.freeze([roleName(auth.role)])

    return {
        pluginId: plugin.id,
        roles,
        hasRole: (role) => typeof role === 'string' && roles().includes(role),
        locale: () => store.locale,
        theme: () => (store.darkMode ? 'dark' : 'light'),
        location: () => pluginAdminLocation(plugin.id, route.fullPath),
        onLocationChange: listeners.location.subscribe,
        onLocaleChange: listeners.locale.subscribe,
        onThemeChange: listeners.theme.subscribe,
        request: async (path, init) => {
            const url = new URL(path, window.location.origin)
            if (
                url.origin !== window.location.origin ||
                url.username ||
                url.password ||
                (url.pathname !== prefix && !url.pathname.startsWith(`${prefix}/`))
            ) {
                throw new Error(t('plugin.requestNamespace'))
            }
            return authFetchRaw(`${url.pathname}${url.search}`, init)
        },
        navigate: async (path = '') => {
            let target: string
            try {
                target = resolvePluginAdminNavigation(plugin.id, route.fullPath, path)
            } catch {
                throw new Error(t('plugin.navigationNamespace'))
            }
            await router.push(target)
        },
        notify: (variance, text) => {
            if (
                ['info', 'success', 'warning', 'error'].includes(variance) &&
                typeof text === 'string' &&
                text.length <= 500
            ) {
                store.msgAlert(variance, text)
            }
        },
    }
}

function listenerError(reason: unknown) {
    console.error(`Plugin '${mountedPluginId ?? 'unknown'}' context listener failed`, reason)
}

function createPluginSubscriptions(): PluginSubscriptions {
    return {
        location: createSubscription(listenerError),
        locale: createSubscription(listenerError),
        theme: createSubscription(listenerError),
    }
}

function clearMountedPlugin() {
    subscriptions?.location.clear()
    subscriptions?.locale.clear()
    subscriptions?.theme.clear()
    subscriptions = undefined
    mountedPluginId = undefined
    container.value?.replaceChildren()
}

async function mountPlugin() {
    const revision = ++mountRevision
    clearMountedPlugin()
    error.value = undefined
    loading.value = true

    const plugin = pluginFromRoute()
    if (!plugin?.admin?.element) {
        error.value = t('plugin.unavailable')
        loading.value = false
        return
    }

    try {
        for (const style of plugin.admin.styles ?? []) {
            loadPluginStyle(assetUrl(plugin, style))
        }
        await loadPluginModule(assetUrl(plugin, plugin.admin.entry ?? ''))
        if (revision !== mountRevision) return
        if (!container.value || !customElements.get(plugin.admin.element)) {
            throw new Error(t('plugin.registrationMissing'))
        }

        const listeners = createPluginSubscriptions()
        const element = document.createElement(plugin.admin.element) as PluginElement
        element.context = contextFor(plugin, listeners)
        subscriptions = listeners
        mountedPluginId = plugin.id
        container.value.replaceChildren(element)
    } catch (reason) {
        if (revision !== mountRevision) return
        container.value?.replaceChildren()
        error.value = reason instanceof Error ? reason.message : t('plugin.loadFailed')
    } finally {
        if (revision === mountRevision) loading.value = false
    }
}

watch(
    () => route.params.pluginId,
    () => void mountPlugin(),
    { immediate: true },
)
watch(
    () => route.fullPath,
    () => {
        if (mountedPluginId === route.params.pluginId) {
            subscriptions?.location.emit(pluginAdminLocation(mountedPluginId, route.fullPath))
        }
    },
)
watch(
    () => store.locale,
    (locale) => subscriptions?.locale.emit(locale),
)
watch(
    () => store.darkMode,
    (darkMode) => subscriptions?.theme.emit(darkMode ? 'dark' : 'light'),
)
onBeforeUnmount(() => {
    mountRevision += 1
    clearMountedPlugin()
})
</script>

<template>
    <div>
        <span v-if="loading" class="loading loading-spinner loading-md" :aria-label="$t('plugin.loading')"></span>
        <div v-else-if="error" role="alert" class="alert alert-error">{{ error }}</div>
        <div ref="container"></div>
    </div>
</template>
