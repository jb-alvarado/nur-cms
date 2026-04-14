<script setup lang="ts">
import { onBeforeMount } from 'vue'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import GenericIndex from '@/components/generic/GenericIndex.vue'

const { t } = useI18n()
const store = useIndex()

store.routeType = 'author'
store.selectAll = false
store.ordering = 'id'

store.visibleRows = [
    { active: true, up: false, name: t('table.id'), field: 'id' },
    { active: false, up: false, name: t('table.firstName'), field: 'first_name' },
    { active: false, up: false, name: t('table.lastName'), field: 'last_name' },
    { active: false, up: false, name: t('table.createdAt'), field: 'created_at' },
]

store.initContent('content/authors', true)

onBeforeMount(() => {
    store.contentSelect()
})
</script>

<template>
    <GenericIndex v-if="store.loaded" />
</template>
