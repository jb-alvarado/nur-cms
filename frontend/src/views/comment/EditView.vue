<script setup lang="ts">
import { useIndex } from '@/stores/index'
import CommentEdit from '@/components/CommentEdit.vue'

const store = useIndex()
</script>
<template>
    <CommentEdit v-if="store.loaded" />
</template>
