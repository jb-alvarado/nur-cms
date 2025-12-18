<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRoute } from 'vue-router'

import AstFormat from '@/components/AstRender.vue'

const route = useRoute()
const entry = ref()

const fetchEntry = async () => {
    try {
        const response = await fetch(`/api/content/entries/article/${route.params.slug}`)

        if (!response.ok) {
            throw new Error(`API error: ${response.status}`)
        }

        entry.value = await response.json()
    } catch (err) {
        console.error('Error fetching entries:', err)
    }
}

onMounted(() => {
    fetchEntry()
})

console.log(route.params.slug)
</script>
<template>
    <div class="flex justify-center p-8 h-full">
        <div v-if="entry" class="max-w-5xl w-full h-full p-4 bg-base-200 rounded">
            <h1 class="text-2xl">{{ entry.title }}</h1>

            <AstFormat :content="entry.ast" />
        </div>
    </div>
</template>
