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

const locales = ref<Array<Locale & { check?: boolean }>>([])
const tsLanguages = ref<string[]>([])
const select = ref(false)
const selectCount = computed(() => locales.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const ordering = ref('id')
const formLocale = ref<Locale>({
    id: 0,
    code: '',
    name: '',
    tsv_dict: '',
})

const deleteModal = ref()
const localeModal = ref()
const isEditing = ref(false)

const localeRows = computed(() => [
    { name: t('table.id'), field: 'id' },
    { name: t('locale.code'), field: 'code' },
    { name: t('locale.name'), field: 'name' },
    { name: t('locale.tsvDict'), field: 'tsv_dict' },
])

async function selectLocales() {
    await fetch(`/api/locales?ordering=${ordering.value}`, {
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
                locales.value = response.results.map((o: any) => ({ check: false, ...o }))
            } else {
                locales.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectTsLanguage() {
    await fetch('/api/ts-language', {
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
                tsLanguages.value = response.results
                    .map((item: any) => item.cfgname)
                    .filter((item: string | undefined) => !!item)
            } else {
                tsLanguages.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

selectLocales()
selectTsLanguage()

function selectAll() {
    for (const item of locales.value) {
        item.check = select.value
    }
}

function editLocale(locale: Locale) {
    formLocale.value = { ...locale }
    isEditing.value = true
    localeModal.value.showModal()
}

function openCreateModal() {
    formLocale.value.id = 0
    formLocale.value.code = ''
    formLocale.value.name = ''
    formLocale.value.tsv_dict = ''
    isEditing.value = false
    localeModal.value.showModal()
}

async function deleteLocale() {
    for (const item of locales.value) {
        if (item.check) {
            await fetch(`/api/locales/${item.id}`, {
                method: 'DELETE',
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    } else {
                        store.msgAlert('success', `Deleted: ${item.code ?? item.id}`)
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e)
                })
        }
    }

    await store.selectLocales()
    await selectLocales()
}

function deselect() {
    for (const locale of locales.value) {
        locale.check = false
    }
}

function saveLocale() {
    const url = isEditing.value ? `/api/locales/${formLocale.value.id}` : `/api/locales`
    const method = isEditing.value ? 'PUT' : 'POST'

    fetch(url, {
        method,
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify({
            code: formLocale.value.code,
            name: formLocale.value.name,
            tsv_dict: formLocale.value.tsv_dict,
        }),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            } else {
                const action = isEditing.value ? 'Updated' : 'Created'
                store.msgAlert('success', `${action} locale: ${formLocale.value.code}`)
                formLocale.value.id = 0
                formLocale.value.code = ''
                formLocale.value.name = ''
                formLocale.value.tsv_dict = ''

                await store.selectLocales()
                await selectLocales()
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
            <div class="grow font-bold">{{ $t('locale.title') }}</div>
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
                        <th v-for="row in localeRows" :key="row.field" class="min-w-16">
                            {{ row.name }}
                        </th>
                        <th class="w-10"></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in locales" :key="i">
                        <th>
                            <label>
                                <input v-model="col.check" type="checkbox" class="checkbox checkbox-sm" />
                            </label>
                        </th>
                        <td v-for="row in localeRows" :key="row.field">
                            {{ (col as any)[row.field] }}
                        </td>
                        <td>
                            <button class="btn btn-sm p-1" @click="editLocale(col)">
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
        :ok-action="deleteLocale"
    >
        <p class="py-4">{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
    </GenericModal>

    <GenericModal
        ref="localeModal"
        :title="isEditing ? $t('dialog.editLocaleTitle') : $t('dialog.createLocaleTitle')"
        :ok-action="saveLocale"
    >
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('locale.code') }}</legend>
            <input v-model="formLocale.code" type="text" class="input w-full" :placeholder="$t('locale.code')" />
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('locale.name') }}</legend>
            <input v-model="formLocale.name" type="text" class="input w-full" :placeholder="$t('locale.name')" />
        </fieldset>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('locale.tsvDict') }}</legend>
            <select
                v-model="formLocale.tsv_dict"
                class="select w-full"
            >
                <option value="" disabled>{{ $t('locale.selectTsvDict') }}</option>
                <option v-for="cfg in tsLanguages" :key="cfg" :value="cfg">{{ cfg }}</option>
            </select>
        </fieldset>
    </GenericModal>
</template>
