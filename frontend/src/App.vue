<script setup lang="ts">
import { onBeforeMount, computed, ref } from 'vue'
import { RouterView, useRoute } from 'vue-router'
import { useHead } from '@unhead/vue'

import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

import AlertMsg from '@/components/AlertMsg.vue'
import MenuSide from '@/components/MenuSide.vue'

const route = useRoute()
const auth = useAuth()
const store = useIndex()

const preferDark = window.matchMedia('(prefers-color-scheme: dark)')?.matches ?? false
const local = localStorage.getItem('language') || 'en'
const theme = ref(localStorage.getItem('theme') || (preferDark ? 'dark' : 'light'))

store.darkMode = theme.value === 'dark'

onBeforeMount(async () => {
    await auth.inspectToken()
})

const showMenu = computed(() => route.meta.showMenu === true && auth.isLogin)
const mainClass = computed(() => (showMenu.value ? 'overflow-y-auto w-full bg-base-100 px-7 pt-3' : 'overflow-y-auto w-full bg-base-100'))

useHead({
    htmlAttrs: {
        lang: computed(() => local),
        'data-theme': computed(() => (store.darkMode ? 'dark' : 'light')),
    },
})
</script>

<template>
    <div class="h-screen bg-base-100">
        <div class="flex flex-row h-full">
            <MenuSide v-if="showMenu" class="pt-3" />
            <main :class="mainClass">
                <RouterView :key="route.fullPath + store.randomKey" />
            </main>
        </div>

        <AlertMsg />
    </div>
</template>
