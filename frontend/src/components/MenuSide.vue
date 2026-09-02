<script setup lang="ts">
import { computed, onBeforeMount } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { locales as appLocales } from '@/i18n'
import { normalizeCode } from '@/utils/helper'
import { menuAllowsRole, pluginAllowsRole, pluginMenuLabel } from '@/types/plugins'

import SseHandler from './SseHandler.vue'

const { t, locale } = useI18n()
const router = useRouter()

const auth = useAuth()
const store = useIndex()

type LangOpt = { code: string; name: string }

onBeforeMount(async () => {
    await store.selectCmsConfiguration()
    await store.selectLocales()
    await store.selectTypes()
    await store.selectPlugins()
    await auth.selectAuthUser()
    auth.obtainUuid()
    store.selectAuthors()

    store.isLoaded = true
})

async function logout() {
    await auth.logout()
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

const languageOptions = computed<LangOpt[]>(() => {
    return appLocales.map((l) => ({
        code: l.language,
        name: l.name,
    }))
})

function setLanguage(code: string) {
    if (store.branding.admin_language) return

    const next = normalizeCode(code)
    locale.value = next
    store.locale = code
    localStorage.setItem('language', code)
    document.documentElement.setAttribute('lang', next)

    store.randomKey = (Math.random() + 1).toString(36).substring(7)
}
</script>

<template>
    <aside class="flex h-full w-38 flex-col bg-base-300 pt-3">
        <div class="flex justify-center">
            <RouterLink class="flex max-w-36 items-center gap-2 text-xl font-bold" to="/">
                <img
                    v-if="store.branding.logo_url"
                    :src="store.branding.logo_url"
                    :alt="store.branding.logo_alt ?? ''"
                    class="size-8 shrink-0 object-contain"
                />
                <span class="truncate">{{ store.branding.frontend_name }}</span>
            </RouterLink>
        </div>
        <div class="flex flex-col justify-center items-center mt-4">
            <div class="join join-vertical mb-2">
                <RouterLink
                    v-if="store.isMenuVisible('authors')"
                    to="/author"
                    class="btn join-item w-31 p-1 justify-normal items-center"
                >
                    <i class="bi bi-person-lines-fill ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.author') }}
                </RouterLink>
                <RouterLink
                    v-if="store.isMenuVisible('categories')"
                    to="/category"
                    class="btn join-item w-31 p-1 justify-normal items-center"
                >
                    <i class="bi bi-boxes ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.category') }}
                </RouterLink>
            </div>
            <div v-if="store.types.length > 0" class="join join-vertical">
                <template v-for="item in store.types" :key="item.id">
                    <RouterLink
                        v-if="store.isMenuVisible(`content:${item.slug}`)"
                        :to="`/content/${item.slug}`"
                        class="btn join-item w-31 p-1 justify-normal items-center"
                        @click="store.routeType = item.slug ?? ''"
                    >
                        <i class="bi ps-0.5 text-2xl leading-0" :class="item.icon"></i>
                        {{ item.name }}
                    </RouterLink>
                </template>

                <RouterLink
                    v-if="store.isMenuVisible('media')"
                    to="/media"
                    class="btn join-item w-31 p-1 justify-normal items-center"
                >
                    <i class="bi bi-card-image ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.media') }}
                </RouterLink>
            </div>
            <div class="mt-2">
                <RouterLink
                    v-if="store.isFeatureEnabled('comments')"
                    to="/comment"
                    class="btn join-item w-31 p-1 justify-normal items-center"
                >
                    <i class="bi bi-chat-left-text ps-0.5 text-2xl leading-0"></i>
                    {{ $t('button.comment') }}
                </RouterLink>
            </div>
            <template
                v-for="plugin in store.plugins.filter((item) => pluginAllowsRole(item, auth.role))"
                :key="plugin.id"
            >
                <div class="plugin-menu-expanded join join-vertical mt-2 rounded border border-base-content/40">
                    <div
                        class="btn btn-disabled join-item w-31 truncate p-1 text-base-content bg-base-300"
                        :title="plugin.name"
                    >
                        {{ plugin.name }}
                    </div>
                    <RouterLink
                        v-for="item in (plugin.admin?.menu ?? []).filter(
                            (item) => plugin.admin && menuAllowsRole(item, plugin.admin, auth.role),
                        )"
                        :key="item.path"
                        :to="item.path"
                        class="btn join-item w-31 p-1 justify-normal items-center"
                    >
                        <i class="bi ps-0.5 text-2xl leading-0" :class="item.icon ?? 'bi-puzzle'"></i>
                        {{ pluginMenuLabel(item, store.locale) }}
                    </RouterLink>
                </div>
                <details class="plugin-menu-dropdown dropdown dropdown-top dropdown-right mt-2">
                    <summary
                        class="btn w-31 list-none justify-between border border-base-content/40 bg-base-300 p-1 text-base-content"
                        :title="plugin.name"
                    >
                        <span class="truncate font-bold">{{ plugin.name }}</span>
                        <i class="bi bi-chevron-right text-sm" />
                    </summary>
                    <ul
                        class="menu dropdown-content max-h-[calc(100vh-2rem)] w-36 overflow-y-auto rounded bg-base-100 p-1 border border-base-content/40 ms-1 -mb-10"
                    >
                        <li
                            v-for="item in (plugin.admin?.menu ?? []).filter(
                                (item) => plugin.admin && menuAllowsRole(item, plugin.admin, auth.role),
                            )"
                            :key="item.path"
                        >
                            <RouterLink :to="item.path" class="justify-normal">
                                <i class="bi text-xl leading-0" :class="item.icon ?? 'bi-puzzle'"></i>
                                {{ pluginMenuLabel(item, store.locale) }}
                            </RouterLink>
                        </li>
                    </ul>
                </details>
            </template>
        </div>
        <div class="grow"></div>
        <div class="flex flex-col justify-center items-center pb-6 gap-2 mt-2">
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
                <div v-if="!store.branding.admin_language" class="dropdown dropdown-top dropdown-center">
                    <div tabindex="0" role="button" class="join-item btn btn-sm p-1.5" :title="t('common.language')">
                        <i class="bi bi-translate text-lg" />
                    </div>
                    <ul tabindex="0" class="dropdown-content menu w-31 rounded-box bg-base-100 p-1.5 shadow">
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
    </aside>
</template>

<style scoped>
.plugin-menu-dropdown {
    display: none;
}

@media (max-height: 800px) {
    .plugin-menu-expanded {
        display: none;
    }

    .plugin-menu-dropdown {
        display: inline-block;
    }
}
</style>
