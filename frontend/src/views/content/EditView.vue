<script setup lang="ts">
import { useIndex } from '@/stores/index'
import GenericEdit from '@/components/generic/GenericEdit.vue'

const store = useIndex()

</script>
<template>
    <GenericEdit v-if="store.loaded" />
</template>
