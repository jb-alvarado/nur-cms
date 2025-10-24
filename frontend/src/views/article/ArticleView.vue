<script setup lang="ts">
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
// import { cloneDeep } from 'lodash-es'
import { ref, computed, nextTick } from 'vue'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

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
const select = ref(false)
// computed selected rows count
const selectCount = computed(() => tableCols.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const published = ref('Publish')

async function articleSelect() {
    const fields = visibleRows.value.map((r: any) => r.field).join(',')

    await fetch(`/api/content/article/?fields=${fields}&limit=${limit.value}&ordering=${ordering.value}`, {
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
            }
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
}

articleSelect()

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

function orderRows(row: any) {
    for (const r of visibleRows.value) {
        if (r.field !== row.field) {
            r.active = false
        }
    }

    row.active = true
    ordering.value = row.up ? row.field : `-${row.field}`

    articleSelect()
}

function selectAll() {
    for (const item of tableCols.value) {
        item.check = select.value
    }

    statusLabel()
}

function formatField(col: any, field: string) {
    if (['created_at', 'updated_at'].includes(field)) {
        return dayjs(col[field] as string).format('llll')
    } else if (field === 'author') {
        return `${col[field]?.first_name} ${col[field]?.last_name}`
    } else {
        return col[field]
    }
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
        <h1 class="text-2xl">{{ $t('article.title') }}</h1>

        <div class="h-10 mt-4 mb-6 flex items-center">
            <div class="grow">
                <div v-if="selectCount > 0">
                    <button class="btn" @click="setStatus()">{{ published }}</button>
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
            <table class="table bg-base-300 table-zebra [&_td]:py-2 rounded-sm">
                <thead>
                    <tr>
                        <th>
                            <label>
                                <input
                                    v-model="select"
                                    type="checkbox"
                                    class="checkbox checkbox-sm"
                                    @change="selectAll"
                                />
                            </label>
                        </th>
                        <th v-for="row in visibleRows" :key="row.field" class="min-w-16">
                            <label class="swap" :class="{ 'text-base-content': row.active }">
                                <input type="checkbox" v-model="row.up" @change="orderRows(row)" />
                                <div class="swap-on">
                                    {{ row.name }}
                                    <i v-if="row.active" class="bi bi-caret-up-fill"></i>
                                </div>
                                <div class="swap-off">
                                    {{ row.name }}
                                    <i v-if="row.active" class="bi bi-caret-down-fill"></i>
                                </div>
                            </label>
                        </th>
                        <th></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in tableCols" :key="i">
                        <th>
                            <label>
                                <input
                                    v-model="col.check"
                                    type="checkbox"
                                    class="checkbox checkbox-sm"
                                    @change="statusLabel()"
                                />
                            </label>
                        </th>
                        <td v-for="row in visibleRows" :key="row.field">
                            <span
                                v-if="(col as any)[row.field] === 'published'"
                                class="text-success bg-base-100 p-1 rounded border"
                            >
                                {{ formatField(col, row.field) }}
                            </span>
                            <span
                                v-else-if="(col as any)[row.field] === 'draft'"
                                class="bg-base-100 p-1 rounded border"
                            >
                                {{ formatField(col, row.field) }}
                            </span>
                            <span v-else>
                                {{ formatField(col, row.field) }}
                            </span>
                        </td>
                        <td>
                            <button class="btn btn-sm p-1">
                                <i class="bi bi-pencil-square text-lg"></i>
                            </button>
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>
</template>
