<script setup lang="ts">
import { useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'

import AuthorEdit from '@/components/AuthorEdit.vue'
import CategoryEdit from '@/components/CategoryEdit.vue'
import GenericEdit from '@/components/GenericEdit.vue'

const route = useRoute()
const store = useIndex()
const typeName = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type

store.typeID = store.types.find(type => type.slug === typeName)?.id ?? 1

</script>
<template>
    <AuthorEdit v-if="typeName === 'author'" />
    <CategoryEdit v-else-if="typeName === 'category'" />
    <GenericEdit v-else />
</template>
