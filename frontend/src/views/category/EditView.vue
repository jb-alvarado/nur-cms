<script setup lang="ts">
import { useIndex } from '@/stores/index'
import EditCategory from '@/components/edit/EditCategory.vue'

const store = useIndex()
</script>
<template>
    <EditCategory v-if="store.loaded" />
</template>
