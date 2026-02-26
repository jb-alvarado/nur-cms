<script setup lang="ts">
import { onBeforeRouteLeave } from 'vue-router'
import { useIndex } from '@/stores/index'
import GenericIndex from '@/components/GenericIndex.vue'

const store = useIndex()

onBeforeRouteLeave((to, from) => {
    if (!from.path.startsWith('/author') || !to.path.startsWith('/author')) {
        store.search = ''
    }
})
</script>

<template>
    <GenericIndex v-if="store.loaded" />
</template>
