<script setup lang="ts">
import { onMounted } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import GenericIndex from '@/components/generic/GenericIndex.vue'

const route = useRoute()
const store = useIndex()
const { t } = useI18n()

store.routeType = (Array.isArray(route.params.type) ? route.params.type[0] : route.params.type) ?? ''
store.selectAll = false

store.type = store.types.filter((i) => i.slug === store.routeType)[0]

store.visibleRows = [
    { active: false, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.title'), field: 'title' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
]

if (store.type.use_meta) {
    store.visibleRows.push(
        { active: true, up: false, name: t('table.startTime'), field: 'start_time' },
        { active: false, up: false, name: t('table.endTime'), field: 'end_time' },
    )
} else {
    store.visibleRows.push({ active: false, up: false, name: t('table.createdAt'), field: 'created_at' })
}

store.visibleRows.push(
    { active: false, up: false, name: t('table.language'), field: 'locale_id' },
    { active: false, up: false, name: t('table.groupId'), field: 'group_id' },
)

store.initContent('content/entries', true)

onMounted(() => {
    store.contentSelect()
})
</script>

<template>
    <GenericIndex v-if="store.loaded" link-prefix="/content" />
</template>
