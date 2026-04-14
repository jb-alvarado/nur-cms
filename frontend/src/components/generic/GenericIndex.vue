<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import { onBeforeRouteUpdate } from 'vue-router'
import { useIndex } from '@/stores/index'
import { useI18n } from 'vue-i18n'
import GenericFilter from '@/components/generic/GenericFilter.vue'
import GenericModal from '@/components/generic/GenericModal.vue'
import GenericPagination from '@/components/generic/GenericPagination.vue'
import GenericTable from '@/components/generic/GenericTable.vue'

const { t } = useI18n()
const store = useIndex()

const props = defineProps<{
    linkPrefix?: string
}>()

onBeforeRouteUpdate((to, from) => {
    if (from.path !== to.path) {
        store.search = ''
    }
})

const lp = computed(() => props.linkPrefix ?? '')

// computed selected rows count
const selectCount = computed(() => store.tableCols.reduce((acc: number, item: any) => acc + (item.check ? 1 : 0), 0))
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
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow capitalize">{{ store.routeType }}</h1>
            <RouterLink
                v-if="store.routeType !== 'comment'"
                :to="`${lp}/${store.routeType}/0`"
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
                    <button v-if="store.search" class="cursor-pointer" @click="resetSearch()">
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

            <GenericFilter v-if="!['author', 'category', 'comment'].includes(store.routeType)" />
        </div>

        <div class="overflow-x-auto mt-4 pb-4">
            <GenericTable :type="store.routeType" :check-box-change="statusLabel" :prefix="lp" />
        </div>
        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="store.contentDelete">
            <p>{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
        </GenericModal>
    </div>
</template>
