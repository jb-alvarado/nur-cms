<script setup lang="ts">
import { onBeforeRouteLeave } from 'vue-router'
import { useIndex } from '@/stores'
import GenericIndex from '@/components/GenericIndex.vue'

const store = useIndex()

onBeforeRouteLeave((to, from) => {
    if (!from.path.startsWith('/content') || !to.path.startsWith('/content')) {
        store.search = ''
    }
})
</script>

<template>
    <GenericIndex v-if="store.loaded" />
</template>
