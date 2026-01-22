<script setup lang="ts">
import { useIndex } from '@/stores/index'
import CategoryEdit from '@/components/CategoryEdit.vue'

const store = useIndex()
</script>
<template>
    <CategoryEdit v-if="store.loaded" />
</template>
