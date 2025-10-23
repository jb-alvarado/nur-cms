<script setup lang="ts">
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const router = useRouter()
const authStore = useAuth()
const indexStore = useIndex()

const menuItems = [
    { icon: 'bi-pencil-square', name: 'Article', link: '/article' },
    { icon: 'bi-file-earmark-text', name: 'Page', link: '/page' },
    { icon: 'bi-card-image', name: 'Media', link: '/media' },
    // { icon: 'bi-card-list', name: 'Blocks', link: '/blocks' },
]

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
    <div class="w-36 h-full bg-base-300 flex flex-col">
        <div class="flex justify-center">
            <RouterLink class="text-2xl font-bold" to="/"> NUR CMS </RouterLink>
        </div>
        <div class="flex justify-center p-6">
            <div class="join join-vertical w-40">
                <RouterLink
                    v-for="item in menuItems"
                    :key="item.name"
                    :to="item.link"
                    class="btn join-item w-28 p-1 justify-normal items-center"
                >
                    <i class="bi p-1 text-xl leading-0" :class="item.icon"></i>
                    {{ item.name }}
                </RouterLink>
            </div>
        </div>
        <div class="grow"></div>
        <div class="flex flex-col justify-center items-center p-3 gap-2">
            <RouterLink
                to="/user"
                class="btn btn-sm bg-accent hover:bg-accent/90 text-accent-content max-w-30 rounded-full p-1 justify-normal items-center"
            >
                <i class="bi bi-person-circle text-xl leading-0"></i>
                <span class="px-1 truncate">{{ authStore.user.first_name }} {{ authStore.user.last_name }}</span>
            </RouterLink>

            <ul class="menu menu-sm menu-horizontal">
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
