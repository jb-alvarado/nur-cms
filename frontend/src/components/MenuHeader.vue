<script setup lang="ts">
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'

import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const router = useRouter()

const authStore = useAuth()
const indexStore = useIndex()

function logout() {
    authStore.removeToken()
    router.push('/')
}

function toggleTheme() {
    indexStore.darkMode = !indexStore.darkMode

    if (indexStore.darkMode) {
        localStorage.setItem('theme', 'dark')
        document.documentElement.setAttribute('data-theme', 'dark')
    } else {
        localStorage.setItem('theme', 'light')
        document.documentElement.setAttribute('data-theme', 'light')
    }
}
</script>

<template>
    <div class="navbar bg-accent text-accent-content min-h-[38px] p-0">
        <RouterLink class="navbar-brand min-w-[120px] p-2 font-bold" to="/">
            NUR CMS
        </RouterLink>
        <div class="navbar-end w-[calc(100%-120px)] flex">
            <ul class="menu menu-sm menu-horizontal px-1">
                <li class="p-0">
                    <label class="swap swap-rotate h-[27px] leading-5">
                        <input
                            type="checkbox"
                            :checked="indexStore.darkMode"
                            @change="toggleTheme"
                            class="focus-within:outline-0!"
                        />
                        <i class="swap-on bi bi-brightness-high text-[18px]"></i>
                        <i class="swap-off bi bi-moon text-[18px]"></i>
                    </label>
                </li>
                <li class="rounded-md p-0">
                    <button class="b h-[27px] leading-5 cursor-pointer" @click="logout()" :title="t('button.logout')">
                        <i class="bi bi-door-closed text-[18px]" />
                    </button>
                </li>
            </ul>
        </div>
    </div>
</template>
