<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { useHead } from '@unhead/vue'
import { locales } from '@/i18n'
import { useRouter } from 'vue-router'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const { locale, t } = useI18n()
const router = useRouter()
const auth = useAuth()
const store = useIndex()

const selectedLang = ref()
const formError = ref('')
const showLoginError = ref(false)
const formPassword = ref('')
const disabled = ref(false)

onMounted(() => {
    selectedLang.value = locales.find((loc: any) => loc.code === locale.value)
})

useHead({
    title: t('button.login'),
})

async function login() {
    disabled.value = true

    try {
        const status = await auth.obtainVerificationCode(formPassword.value)

        if (status === 401 || status === 400 || status === 403) {
            formError.value = t('alert.wrongLogin')
            showLoginError.value = true

            setTimeout(() => {
                showLoginError.value = false
                formError.value = ''
            }, 3000)
        }

        // Only redirect once login succeeded
        if (status === 200 && auth.jwtToken.length < 10) {
            store.msgAlert('success', t('alert.verificationSent'))
            await router.push({ name: 'verification' })
        }

        formPassword.value = ''
    } catch (e) {
        disabled.value = false
        formError.value = e as string
        showLoginError.value = true

        setTimeout(() => {
            showLoginError.value = false
            formError.value = ''
        }, 3000)
    }
}
</script>

<template>
    <div class="w-full h-full flex justify-center items-center">
        <div class="w-96 min-w-full flex flex-col justify-center items-center px-4">
            <h1 class="text-6xl xs:text-8xl">{{ $t('app.title') }}</h1>

            <form class="mt-10" @submit.prevent="login">
                <input
                    v-model="auth.username"
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
                        <button type="submit" class="btn btn-accent" :disabled="disabled">
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
