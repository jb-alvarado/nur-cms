<script setup lang="ts">
import { ref } from 'vue'
import { useHead } from '@unhead/vue'
import { useAuth } from '@/stores/auth'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores'

const { t } = useI18n()
const authStore = useAuth()
const indexStore = useIndex()

const confirmPass = ref('')

useHead({
    title: 'User',
})

async function saveUser() {
    if (authStore.user.password && authStore.user.password !== confirmPass.value) {
        indexStore.msgAlert('error', t('user.mismatch'), 3)
        return
    }

    await authStore.inspectToken()

    await fetch(`/api/auth-user/${authStore.id}`, {
        method: 'PUT',
        headers: { ...indexStore.contentType, ...authStore.authHeader },
        body: JSON.stringify(authStore.user),
    }).then((resp) => {
        if (resp.status === 200) {
            indexStore.msgAlert('success', t('user.updateSuccess'), 3)
        } else {
            indexStore.msgAlert('error', t('user.updateFailed'), 6)
        }
    })
}
</script>

<template>
    <div>
        <h1 class="text-2xl">{{ $t('user.title') }}</h1>
        <form class="w-80 mt-8" @submit.prevent="saveUser">
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('user.name') }}</legend>
                <input v-model="authStore.user.username" type="text" name="username" class="input w-full" />
            </fieldset>

            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('user.firstName') }}</legend>
                <input v-model="authStore.user.first_name" type="text" name="firstName" class="input w-full" />
            </fieldset>
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('user.lastName') }}</legend>
                <input v-model="authStore.user.last_name" type="text" name="lastName" class="input w-full" />
            </fieldset>

            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('user.mail') }}</legend>
                <input v-model="authStore.user.email" type="email" name="mail" class="input w-full" />
            </fieldset>
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('user.newPass') }}</legend>
                <input v-model="authStore.user.password" type="password" name="password" class="input w-full" />
            </fieldset>
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('user.confirmPass') }}</legend>
                <input v-model="confirmPass" type="password" name="password" class="input w-full" />
            </fieldset>

            <div>
                <button class="btn btn-sm btn-accent mt-5" type="submit">{{ $t('user.save') }}</button>
            </div>
        </form>
    </div>
</template>
