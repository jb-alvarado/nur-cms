<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { errMsg } from '@/utils/error'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

import GenericModal from '../GenericModal.vue'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()

const keyInput = ref('')
const templates = ref<NodeTemplateExt[]>([])
const select = ref(false)
const selectCount = computed(() => templates.value.reduce((temp, item: any) => temp + (item.check ? 1 : 0), 0))
const ordering = ref('id')
const template = ref<NodeTemplateExt>({
    id: 0,
    name: '',
    data: {},
})

const deleteModal = ref()
const templateModal = ref()
const isEditing = ref(false)

const templateRows = computed(() => [
    { name: t('table.id'), field: 'id' },
    { name: t('mail.name'), field: 'name' },
])

async function selectTemplates() {
    await fetch(`/api/content/node/templates?ordering=${ordering.value}`, {
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
            if (response.results?.length > 0) {
                templates.value = response.results.map((o: any) => ({ check: false, ...o }))
            } else {
                templates.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

selectTemplates()

function selectAll() {
    for (const item of templates.value) {
        item.check = select.value
    }
}

const addKeyValue = () => {
    if (keyInput.value.trim()) {
        template.value.data[keyInput.value] = ''
        keyInput.value = ''
    }
}

const removeKey = (key: string) => {
    delete template.value.data[key]
}

function editTemplate(node: NodeTemplateExt) {
    // @ts-ignore
    template.value = {
        id: node.id,
        name: node.name,
        data: { ...(node.data as Record<string, any>) },
    }
    isEditing.value = true
    templateModal.value.showModal()
}

function openCreateModal() {
    template.value.id = 0
    template.value.name = ''
    template.value.data = {}
    isEditing.value = false
    templateModal.value.showModal()
}

async function deleteTemplate() {
    for (const item of templates.value) {
        if (item.check) {
            await fetch(`/api/content/node/templates/${item.id}`, {
                method: 'DELETE',
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    } else {
                        store.msgAlert('success', `Deleted: ${item.name ?? item.id}`)
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e)
                })
        }
    }

    await selectTemplates()
}

function deselect() {
    for (const temp of templates.value) {
        temp.check = false
    }
}

function saveTemplate() {
    const url = isEditing.value ? `/api/content/node/templates/${template.value.id}` : `/api/content/node/templates`
    const method = isEditing.value ? 'PUT' : 'POST'

    fetch(url, {
        method,
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            name: template.value.name,
            data: template.value.data,
        }),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            } else {
                const action = isEditing.value ? 'Updated' : 'Created'
                store.msgAlert('success', `${action} template: ${template.value.name}`)
                templateModal.value.close()
                template.value.id = 0
                template.value.name = ''
                template.value.data = {}

                await selectTemplates()
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div class="bg-base-200 p-2 max-w-full border border-base-content/25 rounded-sm">
        <div class="flex">
            <div class="grow font-bold">{{ $t('nodeTemplates.title') }}</div>
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
                        <th v-for="row in templateRows" :key="row.field" class="min-w-16">
                            {{ row.name }}
                        </th>
                        <th class="w-10"></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in templates" :key="i">
                        <th>
                            <label>
                                <input v-model="col.check" type="checkbox" class="checkbox checkbox-sm" />
                            </label>
                        </th>
                        <td v-for="row in templateRows" :key="row.field">
                            {{ (col as any)[row.field] }}
                        </td>
                        <td>
                            <button class="btn btn-sm p-1" @click="editTemplate(col)">
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
        :ok-action="deleteTemplate"
    >
        <p class="py-4">{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
    </GenericModal>
    <GenericModal
        ref="templateModal"
        :title="isEditing ? $t('dialog.editTemplate') : $t('dialog.createTemplate')"
        :ok-action="saveTemplate"
    >
        <div class="flex flex-col gap-4">
            <fieldset class="fieldset py-0">
                <legend class="fieldset-legend">{{ $t('common.name') }}</legend>
                <input v-model="template.name" type="text" class="input w-full" :placeholder="$t('common.name')" />
            </fieldset>

            <div>
                <fieldset class="fieldset py-0 grow">
                    <legend class="fieldset-legend">{{ $t('common.key') }}</legend>
                    <div class="join">
                        <input
                            v-model="keyInput"
                            type="text"
                            class="input grow join-item"
                            :placeholder="$t('common.key')"
                            @keyup.enter="addKeyValue()"
                        />
                        <button class="btn join-item border border-base-content/20" @click="addKeyValue()">
                            <i class="bi bi-plus-lg"></i>
                        </button>
                    </div>
                </fieldset>
            </div>

            <div v-if="Object.keys(template.data).length > 0">
                <h3 class="font-semibold mb-2">{{ $t('common.fields') }}:</h3>
                <div class="flex flex-col gap-2">
                    <fieldset v-for="key in Object.keys(template.data)" :key="key" class="flex w-full join">
                        <input
                            :value="key"
                            type="text"
                            class="input input-sm w-1/3 join-item border border-base-content/20 text-base-content"
                            disabled
                        />
                        <input
                            value="'' ''"
                            type="text"
                            class="input input-sm grow join-item border border-base-content/20"
                            disabled
                        />
                        <button class="btn btn-sm join-item border border-base-content/20" @click="removeKey(key)">
                            <i class="bi bi-x-lg"></i>
                        </button>
                    </fieldset>
                </div>
            </div>
        </div>
    </GenericModal>
</template>
