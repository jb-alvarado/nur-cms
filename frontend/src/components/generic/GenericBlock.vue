<script setup lang="ts">
import { type ModelRef } from 'vue'

const block: ModelRef<Record<string, any> | undefined> = defineModel('block')
</script>
<template>
    <div v-if="block" class="flex flex-col gap-2">
        <div
            v-for="(_, key) in block"
            :key="key"
            class="flex gap-2"
            :class="key === 'text' ? 'items-start' : 'items-center'"
        >
            <label class="min-w-20" :class="{ 'pt-3': key === 'text' }">{{ key }}: </label>
            <textarea v-if="key === 'text'" v-model="block[key]" rows="3" class="textarea grow"></textarea>
            <input v-else v-model="block[key]" type="text" class="input grow" />
        </div>
    </div>
</template>
