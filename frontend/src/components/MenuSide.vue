<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { locales as appLocales } from '@/i18n'

import SseHandler from './SseHandler.vue'

const { t, locale } = useI18n()
const router = useRouter()
const auth = useAuth()
const store = useIndex()

// Load language from localStorage on mount
const savedLang = localStorage.getItem('language')
if (savedLang) {
    locale.value = savedLang
}

auth.selectAuthUser()
auth.obtainUuid()
store.selectAuthors()
store.selectLocales()
store.selectTypes()

function logout() {
    auth.removeToken()
    auth.username = ''
    router.push({ name: 'login' })
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

type LangOpt = { code: string; name: string }
const languageOptions = computed<LangOpt[]>(() => {
    return appLocales.map((l) => ({
        code: l.code,
        name: l.name,
    }))
})

function normalizeCode(code: string) {
    if (!code) return 'en'
    const lower = code.toLowerCase()
    if (lower.startsWith('en')) return 'en'
    if (lower.startsWith('de')) return 'de'
    return lower
}

function setLanguage(code: string) {
    const next = normalizeCode(code)
    locale.value = next
    localStorage.setItem('language', next)
    document.documentElement.setAttribute('lang', next)

    store.randomKey = (Math.random() + 1).toString(36).substring(7)
}
</script>

<template>
    <div class="w-38 h-full bg-base-300 flex flex-col">
        <div class="flex justify-center">
            <RouterLink class="text-xl font-bold" to="/">{{ $t('app.title') }}</RouterLink>
        </div>
        <div class="flex flex-col justify-center items-center mt-4">
            <div class="join join-vertical mb-2">
                <RouterLink to="/author" class="btn join-item w-31 p-1 justify-normal items-center">
                    <i class="bi bi-person-lines-fill ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.author') }}
                </RouterLink>
                <RouterLink to="/category" class="btn join-item w-31 p-1 justify-normal items-center">
                    <i class="bi bi-boxes ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.category') }}
                </RouterLink>
            </div>
            <div v-if="store.types.length > 0" class="join join-vertical">
                <template v-for="item in store.types" :key="item.id">
                    <RouterLink
                        v-if="item.slug === 'event'"
                        to="/content/event"
                        class="btn join-item w-31 p-1 justify-normal items-center"
                        @click="store.routeType = item.slug ?? ''"
                    >
                        <i class="bi ps-0.5 text-2xl leading-0" :class="item.icon"></i>
                        {{ item.name }}
                    </RouterLink>
                    <RouterLink
                        v-else
                        :to="`/content/${item.slug}`"
                        class="btn join-item w-31 p-1 justify-normal items-center"
                        @click="store.routeType = item.slug ?? ''"
                    >
                        <i class="bi ps-0.5 text-2xl leading-0" :class="item.icon"></i>
                        {{ item.name }}
                    </RouterLink>
                </template>

                <RouterLink to="/media" class="btn join-item w-31 p-1 justify-normal items-center">
                    <i class="bi bi-card-image ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.media') }}
                </RouterLink>
            </div>
            <div class="mt-2">
                <RouterLink to="/comment" class="btn join-item w-31 p-1 justify-normal items-center">
                    <i class="bi bi-chat-left-text ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.comment') }}
                </RouterLink>
            </div>
        </div>
        <div class="grow"></div>
        <div class="flex flex-col justify-center items-center pb-6 gap-2">
            <RouterLink
                to="/user"
                class="btn btn-sm bg-accent hover:bg-accent/90 text-accent-content w-31 p-1 justify-normal items-center"
            >
                <i class="bi bi-person-circle text-xl leading-0"></i>
                <span class="px-1 truncate">{{ auth.user.first_name }} {{ auth.user.last_name }}</span>
            </RouterLink>

            <div class="join flex">
                <label class="join-item btn btn-sm swap swap-rotate p-1.5">
                    <input
                        type="checkbox"
                        :checked="store.darkMode"
                        @change="toggleTheme"
                        class="focus-within:outline-0!"
                    />
                    <i class="swap-on bi bi-brightness-high text-lg"></i>
                    <i class="swap-off bi bi-moon text-lg"></i>
                </label>
                <RouterLink
                    v-if="auth.role === 'admin'"
                    to="/configuration"
                    class="join-item btn btn-sm p-1.5"
                    :title="t('button.configure')"
                >
                    <i class="bi bi-gear text-lg" />
                </RouterLink>
                <div class="dropdown dropdown-top">
                    <div tabindex="0" role="button" class="join-item btn btn-sm p-1.5" :title="t('common.language')">
                        <i class="bi bi-translate text-lg" />
                    </div>
                    <ul tabindex="0" class="dropdown-content menu p-1.5 shadow bg-base-100 rounded-box w-40">
                        <li v-for="l in languageOptions" :key="l.code">
                            <button @click="setLanguage(l.code)">{{ l.name }}</button>
                        </li>
                    </ul>
                </div>
                <button class="join-item btn btn-sm p-1.5" @click="logout()" :title="t('button.logout')">
                    <i class="bi bi-door-closed text-lg" />
                </button>
            </div>
        </div>

        <SseHandler v-if="auth.uuid" />
    </div>
</template>
