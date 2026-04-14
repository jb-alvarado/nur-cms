<script setup lang="ts">
import { useIndex } from '@/stores/index'
import EditAuthor from '@/components/edit/EditAuthor.vue'

const store = useIndex()

store.routeType = 'author'
</script>
<template>
    <EditAuthor v-if="store.loaded" />
</template>
