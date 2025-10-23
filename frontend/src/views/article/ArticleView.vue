<script setup lang="ts">
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
// import { cloneDeep } from 'lodash-es'
import { ref, computed } from 'vue'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

dayjs.extend(localizedFormat)

const auth = useAuth()
const store = useIndex()

const allRows = [
    { check: false, name: 'ID', field: 'id' },
    { check: false, name: 'Title', field: 'title' },
    { check: false, name: 'Slug', field: 'slug' },
    { check: false, name: 'Status', field: 'status' },
    { check: false, name: 'Author', field: 'author' },
    { check: false, name: 'Locale', field: 'locale' },
    { check: false, name: 'Created At', field: 'created_at' },
    { check: false, name: 'Updated At', field: 'updated_at' },
]

const storedArticleFields = localStorage.getItem('articleFields')
const visibleRows = ref((storedArticleFields ? JSON.parse(storedArticleFields) : null) || [
    { name: 'ID', field: 'id' },
    { name: 'Title', field: 'title' },
    { name: 'Status', field: 'status' },
    { name: 'Created At', field: 'created_at' },
])

const visibleSet = new Set(visibleRows.value.map((r: any) => r.field))
for (const r of allRows) {
    r.check = visibleSet.has(r.field)
}

const limit = ref(10)
const tableCols = ref<Content[]>([])
const select = ref(false)
// computed selected rows count
const selectCount = computed(() => tableCols.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const published = ref('Publish')

async function articleSelect() {
    const fields = visibleRows.value.map((r) => r.field).join(',')

    await fetch(`/api/content/article/?fields=${fields}&limit=${limit.value}&ordering=id`, {
        headers: auth.authHeader,
    })
        .then((resp) => resp.json())
        .then((response: RespondObj) => {
            if (response.results.length > 0) {
                tableCols.value = response.results.map((o: any) => ({ check: false, ...o }))
            }
        })
}

articleSelect()

async function setStatus() {
    for (const item of tableCols.value) {
        if (item.check) {
            const status = published.value === 'Publish' ? 'published' : 'draft'

            await fetch(`/api/content/content_entries/${item.id}/`, {
                method: 'PUT',
                headers: { ...store.contentType, ...auth.authHeader },
                body: JSON.stringify({ status }),
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        store.msgAlert('error', await resp.text(), 6)
                    } else {
                        store.msgAlert('success', `Update: ${item.id}`, 2)
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
    visibleRows.value = allRows.filter((r) => r.check).map((r) => ({ name: r.name, field: r.field }))

    localStorage.setItem('articleFields', JSON.stringify(visibleRows.value))
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

            <div>
                <div class="dropdown dropdown-end">
                    <div tabindex="0" role="button" class="btn p-2">
                        <i class="bi bi-gear text-xl leading-0"></i>
                    </div>
                    <ul tabindex="-1" class="dropdown-content menu bg-base-300 rounded-sm z-1 w-52 p-2 mt-1 shadow-sm">
                        <li v-for="row in allRows" :key="row.field">
                            <label>
                                <input
                                    v-model="row.check"
                                    type="checkbox"
                                    class="checkbox checkbox-sm"
                                    @change="activeFields"
                                />
                                {{ row.name }}
                            </label>
                        </li>
                    </ul>
                </div>
            </div>
        </div>

        <div class="overflow-x-auto mt-4">
            <table class="table bg-base-300 table-zebra rounded-sm">
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
                        <th v-for="row in visibleRows" :key="row.field">
                            {{ row.name }}
                        </th>
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
                            {{ formatField(col, row.field) }}
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>
</template>
