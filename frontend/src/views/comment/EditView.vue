<script setup lang="ts">
import { useIndex } from '@/stores/index'
import EditComment from '@/components/edit/EditComment.vue'

const store = useIndex()
</script>
<template>
    <EditComment v-if="store.loaded" />
</template>
