<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
// import { cloneDeep } from 'lodash-es'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

import GenericModal from '@/components/GenericModal.vue'
import GenericTable from '@/components/GenericTable.vue'

dayjs.extend(localizedFormat)

const auth = useAuth()
const store = useIndex()

const allRows = [
    { check: true, active: true, up: true, name: 'ID', field: 'id' },
    { check: false, active: false, up: false, name: 'Title', field: 'title' },
    { check: false, active: false, up: false, name: 'Slug', field: 'slug' },
    { check: false, active: false, up: false, name: 'Status', field: 'status' },
    { check: false, active: false, up: false, name: 'Author', field: 'author' },
    { check: false, active: false, up: false, name: 'Locale', field: 'locale' },
    { check: false, active: false, up: false, name: 'Created At', field: 'created_at' },
    { check: false, active: false, up: false, name: 'Updated At', field: 'updated_at' },
]

const storedArticleFields = localStorage.getItem('articleFields')
const visibleRows = ref(
    (storedArticleFields ? JSON.parse(storedArticleFields) : null) || [
        { active: true, up: true, name: 'ID', field: 'id' },
        { active: false, up: false, name: 'Title', field: 'title' },
        { active: false, up: false, name: 'Status', field: 'status' },
        { active: false, up: false, name: 'Created At', field: 'created_at' },
    ]
)

const visibleSet = new Set(visibleRows.value.map((r: any) => r.field))
for (const r of allRows) {
    r.check = visibleSet.has(r.field)
}

const itemLimits = [10, 25, 50, 100]
const limit = ref(localStorage.getItem('articleLimit') ?? 10)
const ordering = ref('id')
const tableCols = ref<Content[]>([])

// computed selected rows count
const selectCount = computed(() => tableCols.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const published = ref('Publish')
const search = ref('')
const deleteModal = ref()
const tableRef = ref()

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

async function articleSelect(sr: string = '') {
    const fields = visibleRows.value.map((r: any) => r.field).join(',')

    const url = sr
        ? `/api/content/article/?fields=${fields}&limit=${limit.value}&ordering=${ordering.value}&search=${sr}`
        : `/api/content/article/?fields=${fields}&limit=${limit.value}&ordering=${ordering.value}`

    await fetch(url, {
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = (await resp.json())?.error ?? (await resp.text())
                throw new Error(msg)
            }
            return resp.json()
        })
        .then((response: RespondObj) => {
            if (response.results?.length > 0) {
                tableCols.value = response.results.map((o: any) => ({ check: false, ...o }))
            } else {
                tableCols.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
}

articleSelect()

async function searchItem() {
    if (search.value.length > 2) {
        articleSelect(search.value)
    } else if (search.value.length === 0) {
        articleSelect()
    }
}

async function setStatus() {
    for (const item of tableCols.value) {
        if (item.check) {
            const status = published.value === 'Publish' ? 'published' : 'draft'

            await fetch(`/api/content/entries/${item.id}/`, {
                method: 'PUT',
                headers: { ...store.contentType, ...auth.authHeader },
                body: JSON.stringify({ status }),
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const json = await resp.json()
                        const msg = json ? json.error : await resp.text()
                        store.msgAlert('error', msg, 6)
                    } else {
                        store.msgAlert('success', `Update: ${item.title ?? item.id}`, 2)
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e, 6)
                })
        }
    }

    await articleSelect()
}

async function deleteArticle() {
    for (const item of tableCols.value) {
        if (item.check) {
            await fetch(`/api/content/entries/${item.id}/`, {
                method: 'DELETE',
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const json = await resp.json()
                        const msg = json ? json.error : await resp.text()
                        store.msgAlert('error', msg, 6)
                    } else {
                        store.msgAlert('success', `Deleted: ${item.title ?? item.id}`, 2)
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e, 6)
                })
        }
    }

    await articleSelect()
}

function activeFields() {
    visibleRows.value = allRows
        .filter((r) => r.check)
        .map((r) => ({ active: r.active, up: r.up, name: r.name, field: r.field }))

    localStorage.setItem('articleFields', JSON.stringify(visibleRows.value))
    articleSelect()
}

function setItemLimit() {
    nextTick(() => {
        localStorage.setItem('articleLimit', limit.value.toString())
        articleSelect()
    })
}

function statusLabel() {
    const selected = tableCols.value.filter((c: any) => c.check)
    if (selected.length === 0) {
        published.value = 'Publish'
        return
    }
    const allPublished = selected.every((c: any) => String(c.status ?? '').toLowerCase() === 'published')
    published.value = allPublished ? 'Unpublish' : 'Publish'
}
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ $t('article.title') }}</h1>
            <button class="btn btn-sm btn-primary text-base">New</button>
        </div>

        <div class="h-10 mt-4 mb-6 flex items-center">
            <div class="grow join">
                <label class="input" :class="selectCount > 0 ? 'w-40' : 'w-74'">
                    <i class="bi bi-search opacity-45"></i>
                    <input v-model="search" type="search" placeholder="Search" @keyup="searchItem" />
                </label>
                <div v-if="selectCount > 0">
                    <button class="btn join-item" @click="setStatus()">{{ published }}</button>
                    <button class="btn text-warning join-item" @click="openDeleteModal">Delete</button>
                    <span class="ms-2">{{ selectCount }} Selected</span>
                </div>
            </div>

            <div class="join">
                <select v-model="limit" class="select join-item" @change="setItemLimit()">
                    <option v-for="lim in itemLimits" :key="lim" :value="lim">{{ lim }}</option>
                </select>
                <div class="join-item dropdown dropdown-end border border-base-content/20">
                    <div tabindex="0" role="button" class="btn p-2 h-9">
                        <i class="bi bi-gear text-lg leading-0"></i>
                    </div>
                    <ul tabindex="-1" class="dropdown-content menu bg-base-300 rounded-sm z-1 w-52 p-2 mt-1 shadow-sm">
                        <li v-for="row in allRows" :key="row.field">
                            <label>
                                <input
                                    v-model="row.check"
                                    type="checkbox"
                                    class="checkbox checkbox-sm"
                                    @change="activeFields"
                                    :disabled="row.field === 'id'"
                                />
                                {{ row.name }}
                            </label>
                        </li>
                    </ul>
                </div>
            </div>
        </div>

        <div class="overflow-x-auto mt-4">
            <GenericTable
                ref="tableRef"
                v-model:ordering="ordering"
                :columns="tableCols"
                :rows="visibleRows"
                :get-items="articleSelect"
                item-route="/article"
                :check-box-change="statusLabel"
            />
        </div>
        <GenericModal ref="deleteModal" title="Delete Selection" :ok-action="deleteArticle">
            <p>Are you sure you want to delete this article{{ selectCount > 1 ? 's' : '' }}?</p>
        </GenericModal>
    </div>
</template>
