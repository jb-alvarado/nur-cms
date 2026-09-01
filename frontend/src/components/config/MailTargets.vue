<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import { authFetch } from '@/composables/authFetch'
import type { MailTarget } from '@/types/models.d'
import type { RespondObj } from '@/types/query.d'

import GenericModal from '@/components/generic/GenericModal.vue'

const { t } = useI18n()
const store = useIndex()

const targets = ref<MailTargetExt[]>([])
const select = ref(false)
const selectCount = computed(() => targets.value.reduce((acc, item) => acc + (item.check ? 1 : 0), 0))
const ordering = ref('id')
const formTarget = ref<MailTargetExt>({
    id: 0,
    name: '',
    subject: '',
    recipients: [],
    allow_html: false,
    allow_dynamic_recipient: false,
})

const deleteModal = ref()
const targetModal = ref()
const isEditing = ref(false)
const recipientInput = ref('')

const targetRows = computed<{ name: string; field: 'id' | 'name' | 'subject' }[]>(() => [
    { name: t('table.id'), field: 'id' },
    { name: t('mail.name'), field: 'name' },
    { name: t('mail.subject'), field: 'subject' },
])

async function selectTargets() {
    await authFetch<RespondObj<MailTarget>>(`/api/contact/targets?ordering=${ordering.value}`)
        .then((response) => {
            if (response.results?.length > 0) {
                targets.value = response.results.map((target) => ({ check: false, ...target }))
            } else {
                targets.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

selectTargets()

function selectAll() {
    for (const item of targets.value) {
        item.check = select.value
    }
}

function addRecipient() {
    if (recipientInput.value.trim()) {
        if (!formTarget.value.recipients.includes(recipientInput.value.trim())) {
            formTarget.value.recipients.push(recipientInput.value.trim())
        }
        recipientInput.value = ''
    }
}

function removeRecipient(index: number) {
    formTarget.value.recipients.splice(index, 1)
}

function editTarget(target: MailTargetExt) {
    formTarget.value = {
        ...target,
        recipients: [...target.recipients],
        allow_dynamic_recipient: target.allow_dynamic_recipient ?? false,
    }
    isEditing.value = true
    targetModal.value.showModal()
}

function openCreateModal() {
    formTarget.value.id = 0
    formTarget.value.name = ''
    formTarget.value.subject = ''
    formTarget.value.recipients = []
    formTarget.value.allow_html = false
    formTarget.value.allow_dynamic_recipient = false
    recipientInput.value = ''
    isEditing.value = false
    targetModal.value.showModal()
}

async function deleteTarget() {
    for (const item of targets.value) {
        if (item.check) {
            await authFetch(`/api/contact/targets/${item.id}`, {
                method: 'DELETE',
            })
                .then(() => {
                    store.msgAlert('success', `Deleted: ${item.name ?? item.id}`)
                })
                .catch((e) => {
                    store.msgAlert('error', e)
                })
        }
    }

    await selectTargets()
}

function deselect() {
    for (const target of targets.value) {
        target.check = false
    }
}

function saveTarget() {
    const url = isEditing.value ? `/api/contact/targets/${formTarget.value.id}` : `/api/contact/targets`
    const method = isEditing.value ? 'PUT' : 'POST'

    authFetch(url, {
        method,
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            name: formTarget.value.name,
            subject: formTarget.value.subject,
            recipients: formTarget.value.recipients,
            allow_html: formTarget.value.allow_html,
            allow_dynamic_recipient: formTarget.value.allow_dynamic_recipient,
        }),
    })
        .then(async () => {
            const action = isEditing.value ? 'Updated' : 'Created'
            store.msgAlert('success', `${action} mail target: ${formTarget.value.name}`)
            formTarget.value.id = 0
            formTarget.value.name = ''
            formTarget.value.subject = ''
            formTarget.value.recipients = []
            formTarget.value.allow_html = false
            formTarget.value.allow_dynamic_recipient = false

            await selectTargets()
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div class="bg-base-200 p-2 border border-base-content/25 rounded-sm w-full md:w-auto">
        <div class="flex">
            <div class="grow font-bold">{{ $t('mail.title') }}</div>
            <button class="btn btn-sm btn-primary text-base" @click="openCreateModal()">{{ $t('button.new') }}</button>
        </div>

        <div class="h-10 flex mt-2 items-center">
            <div class="grow join">
                <div v-if="selectCount > 0">
                    <button class="btn text-warning join-item" @click="deleteModal.showModal()">
                        {{ $t('common.delete') }}
                    </button>
                    <span class="ms-2">{{ selectCount }} {{ $t('common.selected') }}</span>
                </div>
            </div>
        </div>

        <div class="overflow-x-auto mt-4 max-h-96">
            <table class="table bg-base-300 table-pin-rows table-zebra [&_td]:py-2 rounded-sm">
                <thead>
                    <tr>
                        <th class="w-10">
                            <input v-model="select" type="checkbox" class="checkbox checkbox-sm" @change="selectAll" />
                        </th>
                        <th v-for="row in targetRows" :key="row.field" class="min-w-16">
                            {{ row.name }}
                        </th>
                        <th class="w-10"></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in targets" :key="i">
                        <th>
                            <label>
                                <input v-model="col.check" type="checkbox" class="checkbox checkbox-sm" />
                            </label>
                        </th>
                        <td v-for="row in targetRows" :key="row.field">
                            {{ col[row.field] }}
                        </td>
                        <td>
                            <button class="btn btn-sm p-1" @click="editTarget(col)">
                                <i class="bi bi-pencil-square text-lg"></i>
                            </button>
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>

    <GenericModal
        ref="deleteModal"
        :title="$t('dialog.deleteTitle')"
        :cancel-action="deselect"
        :ok-action="deleteTarget"
    >
        <p class="py-4">{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
    </GenericModal>

    <GenericModal
        ref="targetModal"
        :title="isEditing ? $t('dialog.editMailTargetTitle') : $t('dialog.createMailTargetTitle')"
        :ok-action="saveTarget"
    >
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('mail.name') }}</legend>
            <input v-model="formTarget.name" type="text" class="input w-full" :placeholder="$t('mail.name')" />
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('mail.subject') }}</legend>
            <input v-model="formTarget.subject" type="text" class="input w-full" :placeholder="$t('mail.subject')" />
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('mail.recipients') }}</legend>
            <div class="join">
                <input
                    v-model="recipientInput"
                    type="email"
                    class="input join-item w-full"
                    name="email"
                    :placeholder="$t('mail.recipientPlaceholder')"
                    @keyup.enter="addRecipient"
                />
                <button type="button" class="btn join-item border border-base-content/25" @click="addRecipient">
                    {{ $t('button.add') }}
                </button>
            </div>
            <div class="mt-2 flex flex-wrap gap-2">
                <div
                    v-for="(recipient, index) in formTarget.recipients"
                    :key="index"
                    class="badge badge-lg badge-primary pe-0"
                >
                    {{ recipient }}
                    <button type="button" @click="removeRecipient(index)" class="btn btn-ghost btn-xs rounded-box">
                        ✕
                    </button>
                </div>
            </div>
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend flex items-center gap-2">
                <input v-model="formTarget.allow_html" type="checkbox" class="checkbox" />
                {{ $t('mail.allowHtml') }}
            </legend>
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend flex items-center gap-2">
                <input v-model="formTarget.allow_dynamic_recipient" type="checkbox" class="checkbox" />
                {{ $t('mail.allowDynamicRecipient') }}
            </legend>
            <p class="label whitespace-normal">{{ $t('mail.allowDynamicRecipientHint') }}</p>
        </fieldset>
    </GenericModal>
</template>
