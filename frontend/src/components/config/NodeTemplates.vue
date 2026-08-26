<script setup lang="ts">
import { computed, ref } from 'vue'
import { cloneDeep } from 'es-toolkit/object'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import { authFetch } from '@/composables/authFetch'
import type { ContentNodeDataField, ContentNodeDataKind, ContentNodeTemplate } from '@/types/models.d'
import type { RespondObj } from '@/types/query.d'

import GenericModal from '@/components/generic/GenericModal.vue'

type TemplateField = Pick<ContentNodeDataField, 'key' | 'label' | 'kind'> & { default: unknown }
type TemplateDraft = Pick<ContentNodeTemplate, 'id' | 'name'> & {
    data: Record<string, unknown>
    schema: TemplateField[]
}
type SelectableTemplate = TemplateDraft & { check: boolean }

const { t } = useI18n()
const store = useIndex()

const keyInput = ref('')
const templates = ref<SelectableTemplate[]>([])
const select = ref(false)
const selectCount = computed(() => templates.value.reduce((count, item) => count + (item.check ? 1 : 0), 0))
const ordering = ref('id')
const template = ref<TemplateDraft>({
    id: 0,
    name: '',
    data: {},
    schema: [],
})

const deleteModal = ref()
const templateModal = ref()
const isEditing = ref(false)

const templateRows = computed<Array<{ name: string; field: 'id' | 'name' }>>(() => [
    { name: t('table.id'), field: 'id' },
    { name: t('mail.name'), field: 'name' },
])

function kindFromValue(value: unknown): ContentNodeDataKind {
    if (typeof value === 'boolean') return 'boolean'
    if (typeof value === 'number') return 'number'
    if (value !== null && typeof value === 'object') return 'json'
    return 'string'
}

function defaultForKind(kind: ContentNodeDataKind): unknown {
    switch (kind) {
        case 'boolean':
            return false
        case 'number':
            return 0
        case 'json':
            return {}
        default:
            return ''
    }
}

function templateData(): Record<string, unknown> {
    return Object.fromEntries(template.value.schema.map((field) => [field.key, field.default]))
}

async function selectTemplates() {
    try {
        const response = await authFetch<RespondObj<ContentNodeTemplate>>(
            `/api/content/node/templates?ordering=${ordering.value}`,
        )
        templates.value =
            response.results.length > 0
                ? response.results.map((item) => ({
                      check: false,
                      id: item.id,
                      name: item.name,
                      data: item.data && typeof item.data === 'object' && !Array.isArray(item.data) ? item.data : {},
                      schema: item.schema,
                  }))
                : []
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}

selectTemplates()

function selectAll() {
    for (const item of templates.value) item.check = select.value
}

function addField() {
    const key = keyInput.value.trim()
    if (!key || template.value.schema.some((field) => field.key === key)) return

    template.value.schema.push({ key, kind: 'string', default: '' })
    keyInput.value = ''
}

function removeField(index: number) {
    template.value.schema.splice(index, 1)
}

function changeKind(field: TemplateField) {
    field.default = defaultForKind(field.kind)
}

function updateNumberDefault(field: TemplateField, event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value
    field.default = value === '' ? 0 : Number(value)
}

function updateStringDefault(field: TemplateField, event: Event) {
    field.default = (event.currentTarget as HTMLInputElement).value
}

function updateBooleanDefault(field: TemplateField, event: Event) {
    field.default = (event.currentTarget as HTMLInputElement).checked
}

function updateJsonDefault(field: TemplateField, event: Event) {
    const input = event.currentTarget as HTMLTextAreaElement
    try {
        field.default = JSON.parse(input.value)
        input.setCustomValidity('')
    } catch {
        input.setCustomValidity('Invalid JSON')
        input.reportValidity()
    }
}

function jsonValue(value: unknown): string {
    return JSON.stringify(value ?? null, null, 2)
}

function editTemplateByIndex(index: number) {
    const node = templates.value[index]
    if (!node) return

    const schema = node.schema?.length
        ? cloneDeep(node.schema)
        : Object.entries(node.data && typeof node.data === 'object' && !Array.isArray(node.data) ? node.data : {}).map(
              ([key, value]) => ({
                  key,
                  kind: kindFromValue(value),
                  default: cloneDeep(value),
              }),
          )
    template.value = { id: node.id, name: node.name, data: {}, schema }
    isEditing.value = true
    templateModal.value.showModal()
}

function openCreateModal() {
    template.value = { id: 0, name: '', data: {}, schema: [] }
    isEditing.value = false
    templateModal.value.showModal()
}

async function deleteTemplate() {
    for (const item of templates.value) {
        if (!item.check) continue
        try {
            await authFetch(`/api/content/node/templates/${item.id}`, { method: 'DELETE' })
            store.msgAlert('success', `Deleted: ${item.name ?? item.id}`)
        } catch (e) {
            store.msgAlert('error', String(e))
        }
    }

    await selectTemplates()
}

function deselect() {
    for (const temp of templates.value) temp.check = false
}

async function saveTemplate() {
    const url = isEditing.value ? `/api/content/node/templates/${template.value.id}` : '/api/content/node/templates'
    const method = isEditing.value ? 'PUT' : 'POST'

    try {
        await authFetch(url, {
            method,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                name: template.value.name,
                data: templateData(),
                schema: template.value.schema,
            }),
        })
        store.msgAlert('success', `${isEditing.value ? 'Updated' : 'Created'} template: ${template.value.name}`)
        templateModal.value.close()
        await selectTemplates()
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}
</script>

<template>
    <div class="bg-base-200 p-2 border border-base-content/25 rounded-sm w-full md:w-auto">
        <div class="flex">
            <div class="grow font-bold">{{ $t('nodeTemplates.title') }}</div>
            <button class="btn btn-sm btn-primary text-base" @click="openCreateModal">{{ $t('button.new') }}</button>
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
                        <th v-for="row in templateRows" :key="row.field" class="min-w-16">{{ row.name }}</th>
                        <th class="w-10"></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in templates" :key="i">
                        <th><input v-model="col.check" type="checkbox" class="checkbox checkbox-sm" /></th>
                        <td v-for="row in templateRows" :key="row.field">{{ col[row.field] }}</td>
                        <td>
                            <button class="btn btn-sm p-1" @click="editTemplateByIndex(i)">
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

            <fieldset class="fieldset py-0 grow">
                <legend class="fieldset-legend">{{ $t('common.key') }}</legend>
                <div class="join">
                    <input
                        v-model="keyInput"
                        type="text"
                        class="input grow join-item"
                        :placeholder="$t('common.key')"
                        @keyup.enter="addField"
                    />
                    <button class="btn join-item border border-base-content/20" @click="addField">
                        <i class="bi bi-plus-lg"></i>
                    </button>
                </div>
            </fieldset>

            <div v-if="template.schema.length > 0" class="flex flex-col gap-2">
                <h3 class="font-semibold">{{ $t('common.fields') }}:</h3>
                <div v-for="(field, index) in template.schema" :key="index" class="flex items-center gap-1">
                    <input
                        v-model="field.key"
                        type="text"
                        class="input input-sm w-1/4"
                        :aria-label="$t('common.key')"
                    />
                    <select v-model="field.kind" class="select select-sm w-28" @change="changeKind(field)">
                        <option value="string">String</option>
                        <option value="text">Text</option>
                        <option value="boolean">Boolean</option>
                        <option value="number">Number</option>
                        <option value="json">JSON</option>
                    </select>
                    <textarea
                        v-if="field.kind === 'text'"
                        :value="String(field.default ?? '')"
                        rows="2"
                        class="textarea textarea-sm grow"
                        @input="updateStringDefault(field, $event)"
                    ></textarea>
                    <input
                        v-else-if="field.kind === 'boolean'"
                        :checked="field.default === true"
                        type="checkbox"
                        class="checkbox"
                        @change="updateBooleanDefault(field, $event)"
                    />
                    <input
                        v-else-if="field.kind === 'number'"
                        :value="field.default"
                        type="number"
                        class="input input-sm grow"
                        @input="updateNumberDefault(field, $event)"
                    />
                    <textarea
                        v-else-if="field.kind === 'json'"
                        :value="jsonValue(field.default)"
                        rows="3"
                        class="textarea textarea-sm grow font-mono"
                        @change="updateJsonDefault(field, $event)"
                    ></textarea>
                    <input
                        v-else
                        :value="String(field.default ?? '')"
                        type="text"
                        class="input input-sm grow"
                        :placeholder="$t('common.defaultValue')"
                        @input="updateStringDefault(field, $event)"
                    />
                    <button class="btn btn-sm" @click="removeField(index)"><i class="bi bi-x-lg"></i></button>
                </div>
            </div>
        </div>
    </GenericModal>
</template>
