<script setup lang="ts">
import { useIndex } from '@/stores/index'
import AuthorEdit from '@/components/AuthorEdit.vue'

const store = useIndex()
</script>
<template>
    <AuthorEdit v-if="store.loaded" />
</template>
