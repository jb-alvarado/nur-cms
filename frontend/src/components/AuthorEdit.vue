<script setup lang="ts">
import { useRoute } from 'vue-router'
const route = useRoute()
const authorId = Number(route.params.id ?? 0)
</script>
<template><div>Author Editor {{ authorId }}</div></template>
