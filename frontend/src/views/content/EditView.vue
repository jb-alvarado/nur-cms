<script setup lang="ts">
import { useRoute } from 'vue-router'
import AuthorEdit from '@/components/AuthorEdit.vue'
import CategoryEdit from '@/components/CategoryEdit.vue'
import GenericEdit from '@/components/GenericEdit.vue'

const route = useRoute()
const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type
</script>
<template>
    <AuthorEdit v-if="typeParam === 'author'" />
    <CategoryEdit v-else-if="typeParam === 'category'" />
    <GenericEdit v-else />
</template>
