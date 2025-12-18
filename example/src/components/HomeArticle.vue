<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { mediaPath } from '@/utils/helper'
import type { ContentEntrySerializer } from '../../../frontend/src/types/serialized'

interface PaginatedResponse {
    results: ContentEntrySerializer[]
    total: number
    page: number
    page_size: number
    total_pages: number
}

const entries = ref<ContentEntrySerializer[]>([])
const loading = ref(true)
const error = ref<string | null>(null)
const currentPage = ref(1)
const pageSize = ref(6)
const totalPages = ref(1)

const fetchEntries = async (page: number = 0) => {
    try {
        loading.value = true
        error.value = null

        const params = new URLSearchParams({
            offset: page.toString(),
            limit: pageSize.value.toString(),
            locale_id: '2',
            type_id: '1',
            fields: 'id,title,description,slug,media',
            status: 'published',
            ordering: '-published_at',
        })

        const response = await fetch(`/api/content/entries?${params}`)

        if (!response.ok) {
            throw new Error(`API error: ${response.status}`)
        }

        const data: PaginatedResponse = await response.json()
        entries.value = data.results
        currentPage.value = data.page
        totalPages.value = data.total_pages
    } catch (err) {
        error.value = err instanceof Error ? err.message : 'Failed to load entries'
        console.error('Error fetching entries:', err)
    } finally {
        loading.value = false
    }
}

const truncateText = (text: string, length: number = 150): string => {
    if (text.length <= length) return text
    return text.substring(0, length) + '...'
}

const goToPage = (page: number) => {
    fetchEntries(page)
}

onMounted(() => {
    fetchEntries()
})
</script>
<template>
    <h2 class="text-3xl font-bold mb-12 text-center">Latest Articles</h2>

    <!-- Loading State -->
    <div v-if="loading" class="flex justify-center items-center py-16">
        <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600"></div>
    </div>

    <!-- Error State -->
    <div v-else-if="error" class="bg-red-50 border border-red-200 rounded-lg p-6 text-red-700">
        <p class="font-semibold">Error loading content:</p>
        <p>{{ error }}</p>
    </div>

    <!-- Empty State -->
    <div v-else-if="entries.length === 0" class="bg-gray-50 border border-gray-200 rounded-lg p-12 text-center">
        <p class="text-gray-600 text-lg">No articles available yet.</p>
    </div>

    <!-- Content Grid -->
    <div v-else class="flex flex-wrap gap-8 mb-12">
        <article v-for="entry in entries" :key="entry.id!" class="card bg-base-200 w-80 shadow-sm hover:scale-[101%] hover:shadow transition">
            <figure>
                <img :src="mediaPath(entry.media!)" :alt="entry.title!" class="max-h-54" />
            </figure>
            <div class="card-body">
                <h2 class="card-title">{{ entry.title }}</h2>
                <p>
                    {{ truncateText(entry.description!, 120) }}
                </p>
                <div class="card-actions justify-end">
                    <RouterLink :to="`/articles/${entry.slug}`" class="btn btn-primary">Read More</RouterLink>
                </div>
            </div>
        </article>
    </div>

    <!-- Pagination -->
    <div v-if="totalPages > 1" class="flex justify-center items-center gap-2">
        <button
            v-for="page in totalPages"
            :key="page"
            @click="goToPage(page)"
            :class="{
                'px-4 py-2 rounded-lg font-medium transition': true,
                'bg-blue-600 text-white': page === currentPage,
                'bg-white text-gray-700 border border-gray-200 hover:bg-gray-50': page !== currentPage,
            }"
        >
            {{ page }}
        </button>
    </div>
</template>
