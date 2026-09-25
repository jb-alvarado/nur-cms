<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { AuthFetchError, authFetch } from '@/composables/authFetch'
import { useIndex } from '@/stores'
import type { AuthRole, AuthUser } from '@/types/models.d'
import type { RespondObj } from '@/types/query.d'

const { t } = useI18n()
const store = useIndex()

type UserForm = Required<Pick<AuthUser, 'username' | 'first_name' | 'last_name' | 'email' | 'password'>> & {
    role_id: number | null
}

const dialog = ref<HTMLDialogElement>()
const roles = ref<AuthRole[]>([])
const loading = ref(false)
const submitting = ref(false)
const confirmPassword = ref('')
const user = ref<UserForm>(emptyUser())

function emptyUser(): UserForm {
    return {
        username: '',
        first_name: '',
        last_name: '',
        email: '',
        password: '',
        role_id: null,
    }
}

function roleLabel(role: AuthRole): string {
    if (role.name === 'admin') return t('user.admin')
    if (role.name === 'author') return t('user.author')
    if (role.name === 'user') return t('user.member')
    if (typeof role.name === 'object') return role.name.custom

    return role.name
}

async function showModal() {
    if (loading.value || submitting.value) return

    loading.value = true
    try {
        const response = await authFetch<RespondObj<AuthRole>>('/api/auth-role?fields=id,name&limit=200')
        roles.value = response.results.filter((role) => role.id && role.name !== 'guest')
        user.value.role_id = roles.value.find((role) => role.name === 'user')?.id ?? roles.value[0]?.id ?? null

        if (user.value.role_id === null) {
            store.msgAlert('error', t('user.rolesFailed'))
            return
        }

        dialog.value?.showModal()
    } catch {
        store.msgAlert('error', t('user.rolesFailed'))
    } finally {
        loading.value = false
    }
}

function resetForm() {
    user.value = emptyUser()
    confirmPassword.value = ''
}

async function createUser() {
    if (submitting.value) return

    const username = user.value.username.trim()
    const firstName = user.value.first_name.trim()
    const lastName = user.value.last_name.trim()
    const email = user.value.email.trim()

    if (!username || !firstName || !lastName || !email || !user.value.password || !confirmPassword.value) {
        store.msgAlert('error', t('user.requiredFields'))
        return
    }

    if (user.value.password !== confirmPassword.value) {
        store.msgAlert('error', t('user.mismatch'))
        return
    }

    if (new TextEncoder().encode(user.value.password).length > 1024) {
        store.msgAlert('error', t('user.passwordTooLong'))
        return
    }

    if (user.value.role_id === null) return

    submitting.value = true
    try {
        await authFetch<number>('/api/auth-user', {
            method: 'POST',
            headers: store.contentType,
            body: JSON.stringify({
                username,
                first_name: firstName,
                last_name: lastName,
                email,
                password: user.value.password,
                role_id: user.value.role_id,
            } satisfies AuthUser),
        })

        dialog.value?.close()
        store.msgAlert('success', t('user.addSuccess'))
    } catch (error) {
        store.msgAlert(
            'error',
            error instanceof AuthFetchError && error.response.status === 409 ? t('user.loginAlreadyUsed') : t('user.addFailed'),
        )
    } finally {
        submitting.value = false
    }
}

defineExpose({ showModal })
</script>

<template>
    <dialog ref="dialog" class="modal modal-bottom sm:modal-middle" @close="resetForm">
        <div class="modal-box">
            <h2 class="text-lg font-bold">{{ t('user.add') }}</h2>

            <form class="mt-4 flex flex-col gap-2" @submit.prevent="createUser">
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">{{ t('user.name') }}</legend>
                    <input v-model="user.username" type="text" autocomplete="off" maxlength="150" class="input w-full" required />
                </fieldset>
                <div class="grid gap-2 sm:grid-cols-2">
                    <fieldset class="fieldset">
                        <legend class="fieldset-legend">{{ t('user.firstName') }}</legend>
                        <input v-model="user.first_name" type="text" autocomplete="off" maxlength="150" class="input w-full" required />
                    </fieldset>
                    <fieldset class="fieldset">
                        <legend class="fieldset-legend">{{ t('user.lastName') }}</legend>
                        <input v-model="user.last_name" type="text" autocomplete="off" maxlength="150" class="input w-full" required />
                    </fieldset>
                </div>
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">{{ t('user.mail') }}</legend>
                    <input v-model="user.email" type="email" autocomplete="off" maxlength="255" class="input w-full" required />
                </fieldset>
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">{{ t('user.role') }}</legend>
                    <select v-model="user.role_id" class="select w-full" required>
                        <option v-for="role in roles" :key="role.id" :value="role.id">{{ roleLabel(role) }}</option>
                    </select>
                </fieldset>
                <div class="grid gap-2 sm:grid-cols-2">
                    <fieldset class="fieldset">
                        <legend class="fieldset-legend">{{ t('user.password') }}</legend>
                        <input v-model="user.password" type="password" autocomplete="new-password" class="input w-full" required />
                    </fieldset>
                    <fieldset class="fieldset">
                        <legend class="fieldset-legend">{{ t('user.confirmPass') }}</legend>
                        <input v-model="confirmPassword" type="password" autocomplete="new-password" class="input w-full" required />
                    </fieldset>
                </div>

                <div class="modal-action">
                    <button type="button" class="btn" :disabled="submitting" @click="dialog?.close()">
                        {{ t('common.cancel') }}
                    </button>
                    <button type="submit" class="btn btn-accent" :disabled="submitting">
                        {{ t('user.add') }}
                    </button>
                </div>
            </form>
        </div>
    </dialog>
</template>
