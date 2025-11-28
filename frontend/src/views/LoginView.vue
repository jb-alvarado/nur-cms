<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { locales } from '@/i18n'
import { useAuth } from '@/stores/auth'

const { locale, t } = useI18n()
const authStore = useAuth()

const selectedLang = ref()
const formError = ref('')
const showLoginError = ref(false)
const formUsername = ref('')
const formPassword = ref('')

onMounted(() => {
    selectedLang.value = locales.find((loc: any) => loc.code === locale.value)
})

useHead({
    title: t('button.login'),
})

async function login() {
    try {
        const status = await authStore.obtainToken(formUsername.value, formPassword.value)

        formUsername.value = ''
        formPassword.value = ''
        formError.value = ''

        if (status === 401 || status === 400 || status === 403) {
            formError.value = t('alert.wrongLogin')
            showLoginError.value = true

            setTimeout(() => {
                showLoginError.value = false
            }, 3000)
        }

        await authStore.selectAuthUser()
    } catch (e) {
        formError.value = e as string
    }
}
</script>

<template>
    <div class="w-full h-full flex justify-center items-center">
        <div class="w-96 min-w-full flex flex-col justify-center items-center px-4">
            <h1 class="text-6xl xs:text-8xl">{{ $t('app.title') }}</h1>

            <form class="mt-10" @submit.prevent="login">
                <input
                    v-model="formUsername"
                    type="text"
                    name="username"
                    :placeholder="`${$t('user.name')} / ${$t('user.mail')}`"
                    class="input w-full focus:border-base-content/30 focus:outline-base-content/30"
                    required
                />

                <input
                    v-model="formPassword"
                    type="password"
                    name="password"
                    :placeholder="$t('user.password')"
                    class="input w-full mt-5 focus:border-base-content/30 focus:outline-base-content/30"
                    required
                />

                <div class="w-full mt-4 grid grid-flow-row-dense grid-cols-12 grid-rows-1 gap-2">
                    <div class="col-span-3">
                        <button type="submit" class="btn btn-accent">
                            {{ $t('button.login') }}
                        </button>
                    </div>
                    <div class="col-span-12 sm:col-span-9">
                        <div
                            v-if="showLoginError"
                            role="alert"
                            class="alert alert-error w-auto rounded-sm z-2 h-12 p-[0.7rem]"
                        >
                            <i class="bi bi-exclamation-triangle-fill"></i>
                            <span>{{ formError }}</span>
                        </div>
                    </div>
                </div>
            </form>
        </div>
    </div>
</template>
