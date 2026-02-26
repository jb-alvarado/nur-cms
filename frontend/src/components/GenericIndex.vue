<script setup lang="ts">
import { ref, computed, onBeforeMount, nextTick } from 'vue'
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
import { useRoute, useRouter, onBeforeRouteUpdate } from 'vue-router'
import { useIndex } from '@/stores/index'

import { useI18n } from 'vue-i18n'
import GenericFilter from '@/components/GenericFilter.vue'
import GenericModal from '@/components/GenericModal.vue'
import GenericPagination from '@/components/GenericPagination.vue'
import GenericTable from '@/components/GenericTable.vue'

dayjs.extend(localizedFormat)

const route = useRoute()
const router = useRouter()
const store = useIndex()
const { t } = useI18n()

store.selectAll = false
store.routeType = (Array.isArray(route.params.type) ? route.params.type[0] : route.params.type) ?? String(route.name)
const matchedType = store.types.find((type) => type.slug === store.routeType)?.id

store.typeID = matchedType ?? 1
store.ordering = 'id'

onBeforeRouteUpdate((to, from) => {
    if (from.path !== to.path) {
        store.search = ''
    }
})

const authorRows = computed(() => [
    { active: true, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.firstName'), field: 'first_name' },
    { active: false, up: false, name: t('table.lastName'), field: 'last_name' },
    { active: false, up: false, name: t('table.createdAt'), field: 'created_at' },
])

const categoryRows = computed(() => [
    { active: true, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.name'), field: 'name' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
    { active: false, up: false, name: t('table.language'), field: 'locale_id' },
    { active: false, up: false, name: t('table.groupId'), field: 'group_id' },
])

const commentRows = computed(() => [
    { active: false, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.authorName'), field: 'author_name' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
    { active: false, up: false, name: t('table.text'), field: 'text' },
    { active: true, up: false, name: t('table.createdAt'), field: 'created_at' },
])

const eventRows = computed(() => [
    { active: false, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.title'), field: 'title' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
    { active: true, up: false, name: t('table.startTime'), field: 'start_time' },
    { active: false, up: false, name: t('table.endTime'), field: 'end_time' },
    { active: false, up: false, name: t('table.language'), field: 'locale_id' },
])

const defaultEntryRows = computed(() => [
    { active: true, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.title'), field: 'title' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
    { active: false, up: false, name: t('table.createdAt'), field: 'created_at' },
    { active: false, up: false, name: t('table.language'), field: 'locale_id' },
    { active: false, up: false, name: t('table.groupId'), field: 'group_id' },
])

const entryRows = ref([...defaultEntryRows.value])
const linkPrefix = ref('')

onBeforeMount(() => {
    if (store.routeType === 'author') {
        store.visibleRows = authorRows.value
        store.initContent('content/authors', false)
    } else if (store.routeType === 'category') {
        store.visibleRows = categoryRows.value
        store.initContent('content/categories', false)
    } else if (store.routeType === 'comment') {
        store.visibleRows = commentRows.value
        store.initContent('comments', false)
    } else if (store.routeType === 'event') {
        linkPrefix.value = '/content'
        store.visibleRows = eventRows.value
        store.initContent('content/entries')
    } else if (!matchedType) {
        router.push({ name: '404' })
    } else {
        linkPrefix.value = '/content'
        const visibleFields = localStorage.getItem('visibleFields')
        if (visibleFields) {
            entryRows.value = JSON.parse(visibleFields)
        } else {
            entryRows.value = [...defaultEntryRows.value]
        }

        store.visibleRows = entryRows.value
        store.initContent('content/entries')
    }

    store.contentSelect()
})

// computed selected rows count
const selectCount = computed(() => store.tableCols.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const published = ref(t('button.publish'))
const approved = ref(t('button.approve'))
const deleteModal = ref()

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

async function setStatus() {
    let status
    if (store.routeType === 'comment') {
        status =
            approved.value === t('button.approve')
                ? 'approved'
                : approved.value === t('button.pending')
                  ? 'pending'
                  : 'rejected'
    } else {
        status = published.value === t('button.publish') ? 'published' : 'draft'
    }

    store.updateStatus(status)
}

function statusLabel() {
    const selected = store.tableCols.filter((c: any) => c.check)
    if (selected.length === 0) {
        published.value = t('button.publish')
        approved.value = t('button.approve')
        return
    }
    if (store.routeType === 'comment') {
        const allApproved = selected.every((c: any) => String(c.status ?? '').toLowerCase() === 'approved')
        approved.value = allApproved ? t('button.reject') : t('button.approve')
    } else {
        const allPublished = selected.every((c: any) => String(c.status ?? '').toLowerCase() === 'published')
        published.value = allPublished ? t('button.unpublish') : t('button.publish')
    }
}

function onPageChange() {
    nextTick(() => {
        store.contentSelect()
    })
}

function resetSearch() {
    store.search = ''
    store.contentSelect()
}

// TODO: Search result should be saved and recover when navigate to item and back.
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ store.routeType.toLocaleUpperCase() }}</h1>
            <RouterLink
                v-if="store.routeType !== 'comment'"
                :to="`${linkPrefix}/${store.routeType}/0`"
                class="btn btn-sm btn-primary text-base"
            >
                {{ $t('button.new') }}
            </RouterLink>
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
                    <button v-if="store.search" @click="resetSearch()">
                        <i class="bi bi-x-lg" />
                    </button>
                </label>
                <div v-if="selectCount > 0">
                    <button v-if="store.routeType !== 'author'" class="btn join-item" @click="setStatus()">
                        {{ store.routeType === 'comment' ? approved : published }}
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

        <div class="overflow-x-auto mt-4 pb-4">
            <GenericTable :type="store.routeType" :check-box-change="statusLabel" :prefix="linkPrefix" />
        </div>
        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="store.contentDelete">
            <p>{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
        </GenericModal>
    </div>
</template>
