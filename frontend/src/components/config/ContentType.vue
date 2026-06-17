<script setup lang="ts">
import { ref, computed, useTemplateRef, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import { useSortable } from '@vueuse/integrations/useSortable'
import { errMsg } from '@/utils/error'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { slugify } from '@/utils/slugify.js'

import GenericModal from '@/components/generic/GenericModal.vue'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()

const types = ref<ContentTypeExt[]>([])
const select = ref(false)
const selectCount = computed(() => types.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const formType = ref<ContentTypeExt>({
    id: 0,
    name: '',
    slug: '',
    order_index: 0,
    use_meta: false,
})

const deleteModal = ref()
const typeModal = ref()
const typeEL = useTemplateRef('typeEL')
const isEditing = ref(false)
const isSavingOrder = ref(false)
const hasPendingOrderSave = ref(false)

const typeRows = computed(() => [
    { name: t('table.id'), field: 'id' },
    { name: t('contentType.name'), field: 'name' },
    { name: t('contentType.slug'), field: 'slug' },
])

function reindexTypes() {
    types.value.forEach((type, index) => {
        type.order_index = index + 1
    })
}

async function saveTypeOrder() {
    if (isSavingOrder.value) {
        hasPendingOrderSave.value = true
        return
    }

    isSavingOrder.value = true

    try {
        do {
            hasPendingOrderSave.value = false

            const orderedTypes = types.value
                .map((type, index) => ({ id: type.id, order_index: index + 1 }))
                .filter((type) => type.id)

            await Promise.all(
                orderedTypes.map(async (type) => {
                    const resp = await fetch(`/api/content/types/${type.id}`, {
                        method: 'PUT',
                        headers: {
                            ...auth.authHeader,
                            'Content-Type': 'application/json',
                        },
                        body: JSON.stringify({ order_index: type.order_index }),
                    })

                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    }
                }),
            )
        } while (hasPendingOrderSave.value)
    } catch (e) {
        store.msgAlert('error', e instanceof Error ? e.message : String(e))
    } finally {
        isSavingOrder.value = false
    }
}

useSortable(typeEL, types, {
    handle: '.type-drag-handle',
    draggable: '.type-row',
    ghostClass: 'type-row-ghost',
    chosenClass: 'type-row-chosen',
    dragClass: 'type-row-drag',
    onEnd: () => {
        nextTick(async () => {
            reindexTypes()
            await saveTypeOrder()
            store.selectTypes()
        })
    },
} as any)

async function typeSelect() {
    await fetch('/api/content/types?ordering=order_index,id', {
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
    formType.value.use_meta = false
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
    const nextOrderIndex = types.value.reduce((max, type) => Math.max(max, Number(type.order_index) || 0), 0) + 1

    fetch(url, {
        method,
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            name: formType.value.name,
            slug: formType.value.slug,
            use_meta: formType.value.use_meta,
            ...(!isEditing.value ? { order_index: nextOrderIndex } : {}),
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
                formType.value.use_meta = false

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
    <div class="bg-base-200 p-2 border border-base-content/25 rounded-sm">
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

        <div class="overflow-x-auto mt-4">
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
                <tbody ref="typeEL">
                    <tr
                        v-for="(col, i) in types"
                        :key="col.id || i"
                        class="type-row type-drag-handle cursor-grab active:cursor-grabbing"
                    >
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
            <input v-model="formType.slug" type="text" class="input w-full" :placeholder="$t('contentType.slug')" />
        </fieldset>
        <fieldset class="fieldset">
            <label class="label cursor-pointer justify-start gap-2">
                <input v-model="formType.order_index" type="number" step="1" class="input max-w-16" />
                <span class="label-text">{{ t('contentType.order') }}</span>
            </label>
        </fieldset>
        <fieldset class="fieldset">
            <label class="label cursor-pointer justify-start gap-2">
                <input v-model="formType.use_meta" type="checkbox" class="checkbox" />
                <span class="label-text">{{ $t('contentType.useMeta') }}</span>
            </label>
        </fieldset>
    </GenericModal>
</template>

<style scoped>
.type-row {
    transition:
        background-color 0.15s ease,
        opacity 0.15s ease;
}

.type-row-chosen {
    background-color: color-mix(in srgb, var(--color-base-300) 75%, var(--color-primary) 25%);
}

.type-row-ghost {
    opacity: 0.45;
}

.type-row-drag {
    opacity: 0.9;
}
</style>
