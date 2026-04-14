<script setup lang="ts">
import { onBeforeMount } from 'vue'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import GenericIndex from '@/components/generic/GenericIndex.vue'

const { t } = useI18n()
const store = useIndex()

store.routeType = 'comment'
store.selectAll = false
store.ordering = '-created_at'

store.visibleRows = [
    { active: false, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.authorName'), field: 'author_name' },
    { active: false, up: false, name: t('table.status'), field: 'status' },
    { active: false, up: false, name: t('table.text'), field: 'text' },
    { active: true, up: false, name: t('table.createdAt'), field: 'created_at' },
]

store.initContent('comments', true)

onBeforeMount(() => {
    store.contentSelect()
})
</script>

<template>
    <GenericIndex v-if="store.loaded" />
</template>
