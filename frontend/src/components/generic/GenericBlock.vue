<script setup lang="ts">
import { type ModelRef } from 'vue'

const block: ModelRef<Record<string, any> | undefined> = defineModel('block')

</script>
<template>
    <div v-if="block" class="flex flex-col gap-2">
        <div v-for="(_, key) in block" :key="key" class="flex items-center gap-2">
            <label class="min-w-20">{{ key }}: </label>
            <input v-model="block[key]" type="text" class="input grow" />
        </div>
    </div>
</template>
