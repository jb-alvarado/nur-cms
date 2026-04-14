<script setup lang="ts">
import { onBeforeMount } from 'vue'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import GenericIndex from '@/components/generic/GenericIndex.vue'

const { t } = useI18n()
const store = useIndex()

store.routeType = 'event'
store.selectAll = false
store.ordering = 'id'

store.visibleRows = [
    { active: false, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.title'), field: 'title' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
    { active: true, up: false, name: t('table.startTime'), field: 'start_time' },
    { active: false, up: false, name: t('table.endTime'), field: 'end_time' },
    { active: false, up: false, name: t('table.language'), field: 'locale_id' },
    { active: false, up: false, name: t('table.groupId'), field: 'group_id' },
]

store.initContent('content/entries', true)

onBeforeMount(() => {
    store.contentSelect()
})
</script>

<template>
    <GenericIndex v-if="store.loaded" link-prefix="/content" />
</template>
