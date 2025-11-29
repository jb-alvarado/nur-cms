<script setup lang="ts">
import { ref, computed, onMounted, nextTick } from 'vue'
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
import { useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'

import GenericFilter from '@/components/GenericFilter.vue'
import GenericModal from '@/components/GenericModal.vue'
import GenericPagination from '@/components/GenericPagination.vue'
import GenericTable from '@/components/GenericTable.vue'

dayjs.extend(localizedFormat)

const route = useRoute()
const store = useIndex()

store.routeType = (Array.isArray(route.params.type) ? route.params.type[0] : route.params.type) ?? ''

const authorRows = ref([
    { active: true, up: true, name: 'ID', field: 'id' },
    { active: false, up: false, name: 'First Name', field: 'first_name' },
    { active: false, up: false, name: 'Last Name', field: 'last_name' },
    { active: false, up: false, name: 'Created At', field: 'created_at' },
])

const categoryRows = ref([
    { active: true, up: true, name: 'ID', field: 'id' },
    { active: false, up: false, name: 'Name', field: 'name' },
    { active: false, up: false, name: 'Status', field: 'status' },
    { active: false, up: false, name: 'Language', field: 'locale_id' },
    { active: false, up: false, name: 'Group ID', field: 'group_id' },
])

const entryRows = ref([
    { active: true, up: true, name: 'ID', field: 'id' },
    { active: false, up: false, name: 'Title', field: 'title' },
    { active: false, up: false, name: 'Status', field: 'status' },
    { active: false, up: false, name: 'Created At', field: 'created_at' },
    { active: false, up: false, name: 'Language', field: 'locale_id' },
    { active: false, up: false, name: 'Group ID', field: 'group_id' },
])

onMounted(() => {
    if (store.routeType === 'author') {
        store.visibleRows = authorRows.value
        store.initContent('authors', false)
    } else if (store.routeType === 'category') {
        store.visibleRows = categoryRows.value
        store.initContent('categories', false)
    } else {
        const visibleFields = localStorage.getItem('visibleFields')

        if (visibleFields) {
            entryRows.value = JSON.parse(visibleFields)
        }

        store.visibleRows = entryRows.value

        store.initContent('entries')
    }

    store.search = ''
    store.contentSelect()
})

// computed selected rows count
const selectCount = computed(() => store.tableCols.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const published = ref('Publish')
const deleteModal = ref()
const tableRef = ref()

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

async function setStatus() {
    const status = published.value === 'Publish' ? 'published' : 'draft'

    store.updateStatus(status)
}

function statusLabel() {
    const selected = store.tableCols.filter((c: any) => c.check)
    if (selected.length === 0) {
        published.value = 'Publish'
        return
    }
    const allPublished = selected.every((c: any) => String(c.status ?? '').toLowerCase() === 'published')
    published.value = allPublished ? 'Unpublish' : 'Publish'
}

function onPageChange() {
    nextTick(() => {
        store.contentSelect()
        console.log('change page')
    })
}
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ store.routeType.toLocaleUpperCase() }}</h1>
            <RouterLink :to="`/${store.routeType}/0`" class="btn btn-sm btn-primary text-base">New</RouterLink>
        </div>

        <div class="h-10 mt-4 mb-6 flex gap-2 items-center">
            <div class="grow join">
                <label class="input" :class="selectCount > 0 ? 'w-40' : 'w-74'">
                    <i class="bi bi-search opacity-45"></i>
                    <input
                        v-model="store.search"
                        type="search"
                        :placeholder="$t('common.search')"
                        @keyup="store.searchItem"
                    />
                </label>
                <div v-if="selectCount > 0">
                    <button v-if="store.routeType !== 'author'" class="btn join-item" @click="setStatus()">
                        {{ published }}
                    </button>
                    <button class="btn text-warning join-item" @click="openDeleteModal">
                        {{ $t('common.delete') }}
                    </button>
                    <span class="ms-2">{{ selectCount }} {{ $t('common.selected') }}</span>
                </div>
            </div>

            <GenericPagination
                v-model:limit="store.limit"
                v-model:offset="store.offset"
                :total="store.total"
                :page-sizes="store.limits"
                @change="onPageChange"
            />

            <GenericFilter v-if="!['author', 'category'].includes(store.routeType)" />
        </div>

        <div class="overflow-x-auto mt-4">
            <GenericTable ref="tableRef" :type="store.routeType" :check-box-change="statusLabel" />
        </div>
        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="store.contentDelete">
            <p>{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
        </GenericModal>
    </div>
</template>
