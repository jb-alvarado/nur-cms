<script setup lang="ts">
import { onBeforeMount, computed, ref, watch } from 'vue'
import { RouterView, useRoute } from 'vue-router'
import { useHead } from '@unhead/vue'
import { useI18n } from 'vue-i18n'

import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { normalizeCode } from '@/utils/helper'

import AlertMsg from '@/components/AlertMsg.vue'
import MenuSide from '@/components/MenuSide.vue'


const route = useRoute()
const { t } = useI18n()
const auth = useAuth()
const store = useIndex()
const mobileMenuOpen = ref(false)

const preferDark = window.matchMedia('(prefers-color-scheme: dark)')?.matches ?? false
const local = normalizeCode(localStorage.getItem('language') || 'en')
const theme = ref(localStorage.getItem('theme') || (preferDark ? 'dark' : 'light'))

store.darkMode = theme.value === 'dark'

onBeforeMount(async () => {
    await auth.inspectToken()
})

const showMenu = computed(() => route.meta.showMenu === true && auth.isLogin)
const mainClass = computed(() =>
    showMenu.value
        ? 'min-h-0 flex-1 overflow-y-auto bg-base-100 px-4 py-3 sm:px-7'
        : 'h-full w-full overflow-y-auto bg-base-100',
)

watch(
    () => route.fullPath,
    () => {
        mobileMenuOpen.value = false
    },
)

useHead({
    htmlAttrs: {
        lang: computed(() => local),
        'data-theme': computed(() => (store.darkMode ? 'dark' : 'light')),
    },
})
</script>

<template>
    <div class="bg-base-100">
        <div v-if="showMenu" class="drawer h-full md:drawer-open">
            <input id="main-navigation" v-model="mobileMenuOpen" type="checkbox" class="drawer-toggle" />

            <div class="drawer-content flex min-w-0 flex-col">
                <header class="navbar sticky top-0 z-30 min-h-14 shrink-0 border-b border-base-300 bg-base-100 px-3 md:hidden">
                    <label
                        for="main-navigation"
                        class="btn btn-square btn-ghost drawer-button"
                        :aria-label="t('common.navigation')"
                    >
                        <i class="bi bi-list text-2xl" aria-hidden="true"></i>
                    </label>
                    <RouterLink to="/" class="btn btn-ghost text-lg">{{ t('app.title') }}</RouterLink>
                </header>
                <main v-if="store.isLoaded || route.meta.public" :class="mainClass">
                    <RouterView :key="route.fullPath + store.randomKey" />
                </main>
            </div>

            <div class="drawer-side z-40">
                <label for="main-navigation" :aria-label="t('common.closeNavigation')" class="drawer-overlay"></label>
                <MenuSide />
            </div>
        </div>

        <div v-else class="h-full">
            <main v-if="store.isLoaded || route.meta.public" :class="mainClass">
                <RouterView :key="route.fullPath + store.randomKey" />
            </main>
        </div>

        <AlertMsg />
    </div>
</template>
