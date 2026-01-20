<script setup lang="ts">
import { useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'

import CommentEdit from '@/components/CommentEdit.vue'

const route = useRoute()
const store = useIndex()
const typeName = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type

store.typeID = store.types.find((type) => type.slug === typeName)?.id ?? 1
</script>
<template>
    <CommentEdit />
</template>
