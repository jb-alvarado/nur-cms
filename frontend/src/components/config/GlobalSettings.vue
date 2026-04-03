<script setup lang="ts">
import { ref, computed } from 'vue'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import { useI18n } from 'vue-i18n'
import { errMsg } from '@/utils/error'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()

const config = ref<Configuration>()
const configOriginal = ref<Configuration>()

const outputTypeOptions = [
    { value: 'ast', label: 'AST' },
    { value: 'html', label: 'HTML' },
    { value: 'markdown', label: 'Markdown' },
]

const settingsFields = computed(() => [
    { key: 'output_type', label: t('globalSettings.outputType'), type: 'select' },
    { key: 'image_extensions', label: t('globalSettings.imageExtensions'), type: 'text' },
    { key: 'image_resolutions', label: t('globalSettings.imageResolutions'), type: 'text' },
    { key: 'mail_smtp', label: t('globalSettings.mailSmtp'), type: 'text' },
    { key: 'mail_port', label: t('globalSettings.mailPort'), type: 'number' },
    { key: 'mail_user', label: t('globalSettings.mailUser'), type: 'text' },
    { key: 'mail_password', label: t('globalSettings.mailPassword'), type: 'password' },
    { key: 'mail_starttls', label: t('globalSettings.mailStarttls'), type: 'checkbox' },
    { key: 'notification_emails', label: t('globalSettings.notificationEmails'), type: 'text' },
])

async function configSelect() {
    await fetch(`/api/configuration`, {
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            return resp.json()
        })
        .then((response: RespondObj) => {
            if (response) {
                config.value = response
                configOriginal.value = cloneDeep(response)
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

configSelect()

function configUpdate() {
    if (!config.value) return

    const payload = Object.fromEntries(
        Object.entries(config.value).filter(([key, value]) => {
            if (!configOriginal.value) return true
            return !isEqual(value, (configOriginal.value as Record<string, unknown>)[key])
        })
    )

    if (payload.image_extensions && typeof payload.image_extensions === 'string') {
        payload.image_extensions = payload.image_extensions.split(/[,;]/).map(e => e.trim())
    }

    if (payload.image_resolutions && typeof payload.image_resolutions === 'string') {
        payload.image_resolutions = payload.image_resolutions.split(/[,;]/).map(e => Number(e.trim()))
    }

    if (payload.notification_emails && typeof payload.notification_emails === 'string') {
        payload.notification_emails = payload.notification_emails.split(/[,;]/).map(e => e.trim())
    }

    fetch('/api/configuration', {
        method: 'PUT',
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            } else {
                store.msgAlert('success', 'Update configuration')

                await configSelect()
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div v-if="config" class="bg-base-200 p-2 border border-base-content/25 rounded-sm max-w-xl">
        <div class="flex mb-4">
            <div class="grow font-bold">{{ $t('globalSettings.title') }}</div>
            <button class="btn btn-sm btn-primary text-base" @click="configUpdate()">{{ $t('button.save') }}</button>
        </div>

        <div class="overflow-x-auto">
            <table class="table bg-base-300 table-zebra">
                <tbody>
                    <tr v-for="field in settingsFields" :key="field.key">
                        <td class="font-semibold">{{ field.label }}</td>
                        <td class="min-w-46 md:min-w-86">
                            <input
                                v-if="field.type === 'checkbox'"
                                v-model="(config as any)[field.key]"
                                type="checkbox"
                                class="checkbox"
                            />
                            <select
                                v-else-if="field.type === 'select'"
                                v-model="(config as any)[field.key]"
                                class="select select-bordered w-full max-w-xs"
                            >
                                <option v-for="opt in outputTypeOptions" :key="opt.value" :value="opt.value">
                                    {{ opt.label }}
                                </option>
                            </select>
                            <input
                                v-else
                                v-model="(config as any)[field.key]"
                                :type="field.type"
                                :name="field.key"
                                class="input input-bordered w-full max-w-xs"
                                :placeholder="field.label"
                            />
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>
</template>
