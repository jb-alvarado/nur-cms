<script setup lang="ts">
import { type ModelRef } from 'vue'

const block: ModelRef<Record<string, any> | undefined> = defineModel('block')
const orderIndex: ModelRef<Record<string, any> | undefined> = defineModel('index')

const emit = defineEmits(['sort'])

</script>
<template>
    <div v-if="block" class="flex flex-col gap-2">
        <div v-for="(_, key) in block" :key="key" class="flex items-center gap-2">
            <label class="min-w-20">{{ key }}: </label>
            <input v-model="block[key]" type="text" class="input grow" />
            <input v-model="orderIndex" type="number" class="input w-15" @change="emit('sort')" />
        </div>
    </div>
</template>
