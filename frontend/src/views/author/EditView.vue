<script setup lang="ts">
import { useIndex } from '@/stores/index'
import EditAuthor from '@/components/edit/EditAuthor.vue'

const store = useIndex()
</script>
<template>
    <EditAuthor v-if="store.loaded" />
</template>
