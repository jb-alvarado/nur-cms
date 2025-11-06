<script setup lang="ts">
import { ref, computed } from 'vue'
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
import { useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'

import GenericFilter from '@/components/GenericFilter.vue'
import GenericModal from '@/components/GenericModal.vue'
import GenericTable from '@/components/GenericTable.vue'

dayjs.extend(localizedFormat)

const route = useRoute()
const store = useIndex()

const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type

store.initContent(typeParam ?? '')
store.search = ''
store.contentSelect()

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
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ typeParam }}</h1>
            <button class="btn btn-sm btn-primary text-base">New</button>
        </div>

        <div class="h-10 mt-4 mb-6 flex items-center">
            <div class="grow join">
                <label class="input" :class="selectCount > 0 ? 'w-40' : 'w-74'">
                    <i class="bi bi-search opacity-45"></i>
                    <input v-model="store.search" type="search" placeholder="Search" @keyup="store.searchItem" />
                </label>
                <div v-if="selectCount > 0">
                    <button class="btn join-item" @click="setStatus()">{{ published }}</button>
                    <button class="btn text-warning join-item" @click="openDeleteModal">Delete</button>
                    <span class="ms-2">{{ selectCount }} Selected</span>
                </div>
            </div>

            <GenericFilter />
        </div>

        <div class="overflow-x-auto mt-4">
            <GenericTable
                ref="tableRef"
                v-model:ordering="store.ordering"
                :columns="store.tableCols"
                :rows="store.visibleRows"
                :type="typeParam"
                :check-box-change="statusLabel"
            />
        </div>
        <GenericModal ref="deleteModal" title="Delete Selection" :ok-action="store.contentDelete">
            <p>Are you sure you want to delete this {{ typeParam }}{{ selectCount > 1 ? 's' : '' }}?</p>
        </GenericModal>
    </div>
</template>
