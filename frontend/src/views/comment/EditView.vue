<script setup lang="ts">
import { useIndex } from '@/stores/index'
import EditComment from '@/components/edit/EditComment.vue'

const store = useIndex()

store.routeType = 'comment'
</script>
<template>
    <EditComment v-if="store.loaded" />
</template>
