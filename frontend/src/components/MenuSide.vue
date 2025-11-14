<script setup lang="ts">
import { onBeforeMount } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const router = useRouter()
const auth = useAuth()
const store = useIndex()

onBeforeMount(async () => {
    store.selectAuthors()
    await store.selectLocales()
    await store.selectTypes()

    for (const type of store.types) {
        if (type.name === 'Article') {
            type.icon = 'bi-card-list'
        } else if (type.name === 'Page') {
            type.icon = 'bi-card-heading'
        } else if (type.name === 'Event') {
            type.icon = 'bi-calendar-event'
        } else {
            type.icon = 'bi-card-text'
        }
    }
})

function logout() {
    auth.removeToken()
    router.push('/')
}

function toggleTheme() {
    store.darkMode = !store.darkMode

    if (store.darkMode) {
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
        <div class="flex flex-col justify-center p-6">
            <div class="join join-vertical w-40 mb-2">
                <RouterLink to="/author" class="btn join-item w-28 p-1 justify-normal items-center ">
                    <i class="bi bi-person-lines-fill p-1 text-2xl leading-0"></i>
                    Author
                </RouterLink>
                <RouterLink to="/category" class="btn join-item w-28 p-1 justify-normal items-center">
                    <i class="bi bi-boxes p-1 text-2xl leading-0"></i>
                    Category
                </RouterLink>
            </div>
            <div class="join join-vertical w-40">
                <RouterLink
                    v-for="item in store.types"
                    :key="item.name"
                    :to="`/${item.slug}`"
                    class="btn join-item w-28 p-1 justify-normal items-center"
                >
                    <i class="bi p-1 text-2xl leading-0" :class="item.icon"></i>
                    {{ item.name }}
                </RouterLink>
                <RouterLink to="/media" class="btn join-item w-28 p-1 justify-normal items-center">
                    <i class="bi bi-card-image p-1 text-2xl leading-0"></i>
                    Media
                </RouterLink>
            </div>
        </div>
        <div class="grow"></div>
        <div class="flex flex-col justify-center items-center p-3 gap-2">
            <RouterLink
                to="/user"
                class="btn btn-sm bg-accent hover:bg-accent/90 text-accent-content w-27 p-1 justify-normal items-center"
            >
                <i class="bi bi-person-circle text-xl leading-0"></i>
                <span class="px-1 truncate">{{ auth.user.first_name }} {{ auth.user.last_name }}</span>
            </RouterLink>

            <div class="join flex">
                <label class="join-item btn btn-sm swap swap-rotate p-2">
                    <input
                        type="checkbox"
                        :checked="store.darkMode"
                        @change="toggleTheme"
                        class="focus-within:outline-0!"
                    />
                    <i class="swap-on bi bi-brightness-high text-lg"></i>
                    <i class="swap-off bi bi-moon text-lg"></i>
                </label>
                <RouterLink to="/configuration" class="join-item btn btn-sm p-2" :title="t('button.configure')">
                    <i class="bi bi-gear text-lg" />
                </RouterLink>
                <button class="join-item btn btn-sm p-2" @click="logout()" :title="t('button.logout')">
                    <i class="bi bi-door-closed text-lg" />
                </button>
            </div>
        </div>
    </div>
</template>
