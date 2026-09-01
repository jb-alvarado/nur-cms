<script setup lang="ts">
import { onBeforeMount, computed, ref, watch } from 'vue'
import { RouterView, useRoute } from 'vue-router'
import { useHead } from '@unhead/vue'
import { useI18n } from 'vue-i18n'

import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { normalizeCode } from '@/utils/helper'
import { pluginViewKey } from '@/utils/pluginAdmin'

import AlertMsg from '@/components/AlertMsg.vue'
import MenuSide from '@/components/MenuSide.vue'

const route = useRoute()
const { t, locale } = useI18n()
const auth = useAuth()
const store = useIndex()
const mobileMenuOpen = ref(false)

const preferDark = window.matchMedia('(prefers-color-scheme: dark)')?.matches ?? false
const local = normalizeCode(localStorage.getItem('language') || 'en')
const theme = ref(localStorage.getItem('theme') || (preferDark ? 'dark' : 'light'))

store.darkMode = theme.value === 'dark'
locale.value = local
store.locale = local

onBeforeMount(async () => {
    await store.selectBranding()
    await auth.inspectToken()
})

const showMenu = computed(() => route.meta.showMenu === true && auth.isLogin)
const mainClass = computed(() =>
    showMenu.value
        ? 'min-h-0 flex-1 overflow-y-auto bg-base-100 px-4 py-3 sm:px-7'
        : 'h-full w-full overflow-y-auto bg-base-100',
)
const routerViewKey = computed(() => {
    if (route.name === 'plugin admin') {
        const pluginId = typeof route.params.pluginId === 'string' ? route.params.pluginId : ''
        return pluginViewKey(pluginId)
    }
    return `${route.fullPath}:${store.randomKey}`
})

watch(
    () => route.fullPath,
    () => {
        mobileMenuOpen.value = false
    },
)

watch(
    () => store.branding.admin_language,
    (configuredLanguage) => {
        const next = normalizeCode(configuredLanguage ?? localStorage.getItem('language') ?? 'en')
        if (locale.value === next) return

        locale.value = next
        store.locale = next
        store.randomKey = (Math.random() + 1).toString(36).substring(7)
    },
    { immediate: true },
)

useHead({
    titleTemplate: (title?: string) => {
        const frontendName = store.branding.frontend_name
        return !title || title === 'NUR CMS' || title === frontendName ? frontendName : `${title} | ${frontendName}`
    },
    htmlAttrs: {
        lang: computed(() => locale.value),
        'data-theme': computed(() => (store.darkMode ? 'dark' : 'light')),
    },
})
</script>

<template>
    <div class="bg-base-100">
        <div v-if="showMenu" class="drawer h-full md:drawer-open">
            <input id="main-navigation" v-model="mobileMenuOpen" type="checkbox" class="drawer-toggle" />

            <div class="drawer-content flex min-w-0 flex-col">
                <header
                    class="navbar sticky top-0 z-30 min-h-14 shrink-0 border-b border-base-300 bg-base-100 px-3 md:hidden"
                >
                    <label
                        for="main-navigation"
                        class="btn btn-square btn-ghost drawer-button"
                        :aria-label="t('common.navigation')"
                    >
                        <i class="bi bi-list text-2xl" aria-hidden="true"></i>
                    </label>
                    <RouterLink to="/" class="btn btn-ghost max-w-[calc(100%-3rem)] truncate text-lg">
                        {{ store.branding.frontend_name }}
                    </RouterLink>
                </header>
                <main v-if="store.isLoaded || route.meta.public" :class="mainClass">
                    <RouterView :key="routerViewKey" />
                </main>
            </div>

            <div class="drawer-side z-40">
                <label for="main-navigation" :aria-label="t('common.closeNavigation')" class="drawer-overlay"></label>
                <MenuSide />
            </div>
        </div>

        <div v-else class="h-full">
            <main v-if="store.isLoaded || route.meta.public" :class="mainClass">
                <RouterView :key="routerViewKey" />
            </main>
        </div>

        <AlertMsg />
    </div>
</template>
