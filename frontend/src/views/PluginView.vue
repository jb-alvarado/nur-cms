<script lang="ts">
const modules = new Map<string, Promise<void>>()
const styles = new Map<string, HTMLLinkElement>()
</script>

<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

import { authFetchRaw } from '@/composables/authFetch'
import { useIndex } from '@/stores/index'
import type { PluginAdminContext, PluginMetadata } from '@/types/plugins'

type PluginElement = HTMLElement & { context?: PluginAdminContext }

const route = useRoute()
const router = useRouter()
const store = useIndex()
const { t } = useI18n()
const container = ref<HTMLElement>()
const error = ref<string>()
const loading = ref(true)
let mountRevision = 0

function pluginFromRoute(): PluginMetadata | undefined {
    const id = typeof route.params.pluginId === 'string' ? route.params.pluginId : ''
    return store.plugins.find((plugin) => plugin.id === id && plugin.admin?.entry && plugin.admin.element)
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

function contextFor(plugin: PluginMetadata): PluginAdminContext {
    const prefix = `/api/plugins/${plugin.id}`
    const adminPath = `/admin/plugins/${plugin.id}`

    return {
        pluginId: plugin.id,
        locale: () => store.locale,
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
            const base = new URL(`${adminPath}/`, window.location.origin)
            const target = path ? new URL(path, path.startsWith('/') ? window.location.origin : base) : base
            if (
                target.origin !== window.location.origin ||
                target.username ||
                target.password ||
                (target.pathname !== adminPath && !target.pathname.startsWith(`${adminPath}/`))
            ) {
                throw new Error(t('plugin.navigationNamespace'))
            }
            await router.push(`${target.pathname}${target.search}${target.hash}`)
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

async function mountPlugin() {
    const revision = ++mountRevision
    container.value?.replaceChildren()
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

        const element = document.createElement(plugin.admin.element) as PluginElement
        element.context = contextFor(plugin)
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
    () => route.fullPath,
    () => void mountPlugin(),
    { immediate: true },
)
onBeforeUnmount(() => {
    mountRevision += 1
    container.value?.replaceChildren()
})
</script>

<template>
    <div>
        <span v-if="loading" class="loading loading-spinner loading-md" :aria-label="$t('plugin.loading')"></span>
        <div v-else-if="error" role="alert" class="alert alert-error">{{ error }}</div>
        <div ref="container"></div>
    </div>
</template>
