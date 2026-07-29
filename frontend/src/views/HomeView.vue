<script setup lang="ts">
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
import 'dayjs/locale/de'
import 'dayjs/locale/en'
import { ref, onMounted, watch } from 'vue'
import { useHead } from '@unhead/vue'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { authFetch } from '@/composables/authFetch'

dayjs.extend(localizedFormat)

const { t, locale } = useI18n()
const store = useIndex()
const auth = useAuth()
const frontendName = __FRONTEND_NAME__

const lastUsers = ref<AuthUser[]>([])

// Set dayjs locale based on i18n locale
watch(
    () => locale.value,
    (newLocale) => {
        dayjs.locale(newLocale)
    },
    { immediate: true },
)

async function selectLatestLogins() {
    await authFetch<RespondObj>(
        '/api/auth-user?last_login=true&fields=id,first_name,last_name,last_login&ordering=-last_login&limit=5',
    )
        .then((response: RespondObj) => {
            lastUsers.value = response.results
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

onMounted(() => {
    if (auth.role === 'admin') {
        selectLatestLogins()
    }
})

useHead({
    title: t('button.home'),
})
</script>

<template>
    <div class="relative h-full w-full overflow-hidden">
        <div class="w-full h-full flex items-center justify-center">
            <div class="flex flex-col items-center text-center opacity-30">
                <div id="homeLogo" class="size-64 sm:size-96" />
                <h1 class="mt-3 text-3xl font-bold sm:mt-5 sm:text-5xl">{{ frontendName }}</h1>
            </div>
        </div>
        <div v-if="lastUsers.length > 0" class="absolute bg-base-200 z-10 top-5 right-2 p-3 rounded-md">
            <div class="font-bold text-lg mb-1">{{ t('home.lastLogins') }}</div>
            <div v-for="user in lastUsers" :key="user.id!">
                {{ user.first_name }}
                {{ user.last_name }}
                <span class="text-base-content/60">{{ dayjs(user.last_login).format('llll') }}</span>
            </div>
        </div>
    </div>
</template>
