<script setup lang="ts">
import { useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'
import GenericEdit from '@/components/generic/GenericEdit.vue'

const route = useRoute()
const store = useIndex()

store.routeType = (Array.isArray(route.params.type) ? route.params.type[0] : route.params.type) ?? ''
</script>
<template>
    <GenericEdit v-if="store.loaded" />
</template>
