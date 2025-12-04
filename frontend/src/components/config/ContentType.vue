<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { errMsg } from '@/utils/error'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { slugify } from '@/utils/slugify.js'

import GenericModal from '../GenericModal.vue'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()

const types = ref<ContentTypeExt[]>([])
const select = ref(false)
const selectCount = computed(() => types.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const ordering = ref('id')
const formType = ref<ContentTypeExt>({
    id: 0,
    name: '',
    slug: '',
})

const deleteModal = ref()
const typeModal = ref()
const isEditing = ref(false)

const typeRows = computed(() => [
    { name: t('table.id'), field: 'id' },
    { name: t('contentType.name'), field: 'name' },
    { name: t('contentType.slug'), field: 'slug' },
])

async function typeSelect() {
    await fetch(`/api/content/types?ordering=${ordering.value}`, {
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
                types.value = response.results.map((o: any) => ({ check: false, ...o }))
            } else {
                types.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

typeSelect()

function selectAll() {
    for (const item of types.value) {
        item.check = select.value
    }
}

function updateSlug() {
    formType.value.slug = slugify(formType.value.name!)
}

function editType(type: ContentTypeExt) {
    formType.value = { ...type }
    isEditing.value = true
    typeModal.value.showModal()
}

function openCreateModal() {
    formType.value.id = 0
    formType.value.name = ''
    formType.value.slug = ''
    isEditing.value = false
    typeModal.value.showModal()
}

async function deleteType() {
    for (const item of types.value) {
        if (item.check) {
            await fetch(`/api/content/types/${item.id}`, {
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

    store.selectTypes()
    await typeSelect()
}

function deselect() {
    for (const type of types.value) {
        type.check = false
    }
}

function saveType() {
    const url = isEditing.value ? `/api/content/types/${formType.value.id}` : `/api/content/types`
    const method = isEditing.value ? 'PUT' : 'Post'

    fetch(url, {
        method,
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            name: formType.value.name,
            slug: formType.value.slug,
        }),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            } else {
                const action = isEditing.value ? 'Updated' : 'Created'
                store.msgAlert('success', `${action} type: ${formType.value.name}`)
                formType.value.id = 0
                formType.value.name = ''
                formType.value.slug = ''

                store.selectTypes()
                await typeSelect()
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div class="bg-base-200 mt-4 p-2 max-w-96 border border-base-content/25 rounded-sm">
        <div class="flex">
            <div class="grow font-bold">{{ $t('contentType.title') }}</div>
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

        <div class="overflow-x-auto mt-4 max-h-52">
            <table class="table bg-base-300 table-pin-rows table-zebra [&_td]:py-2 rounded-sm">
                <thead>
                    <tr>
                        <th class="w-10">
                            <input v-model="select" type="checkbox" class="checkbox checkbox-sm" @change="selectAll" />
                        </th>
                        <th v-for="row in typeRows" :key="row.field" class="min-w-16">
                            {{ row.name }}
                        </th>
                        <th class="w-10"></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in types" :key="i">
                        <th>
                            <label>
                                <input v-model="col.check" type="checkbox" class="checkbox checkbox-sm" />
                            </label>
                        </th>
                        <td v-for="row in typeRows" :key="row.field">
                            {{ (col as any)[row.field] }}
                        </td>
                        <td>
                            <button class="btn btn-sm p-1" @click="editType(col)">
                                <i class="bi bi-pencil-square text-lg"></i>
                            </button>
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>

    <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :cancel-action="deselect" :ok-action="deleteType">
        <p class="py-4">{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
    </GenericModal>

    <GenericModal
        ref="typeModal"
        :title="isEditing ? $t('dialog.editTypeTitle') : $t('dialog.createTypeTitle')"
        :ok-action="saveType"
    >
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('contentType.name') }}</legend>
            <input
                v-model="formType.name"
                type="text"
                class="input w-full"
                :placeholder="$t('contentType.name')"
                @input="updateSlug()"
            />
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('contentType.slug') }}</legend>
            <input
                v-model="formType.slug"
                type="text"
                class="input w-full"
                :placeholder="$t('contentType.slug')"
            />
        </fieldset>
    </GenericModal>
</template>
