<script setup lang="ts">
import { useIndex } from '@/stores/index'
import GenericIndex from '@/components/generic/GenericIndex.vue'

const store = useIndex()
</script>

<template>
    <GenericIndex v-if="store.loaded" />
</template>
