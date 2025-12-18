<script setup lang="ts">
import { ref, onMounted } from 'vue'
import type { Ref } from 'vue'

import type { ContentEntrySerializer } from '../../../frontend/src/types/serialized'

interface PaginatedResponse {
    results: ContentEntrySerializer[]
    total: number
    page: number
    page_size: number
    total_pages: number
}

const entries: Ref<ContentEntrySerializer[]> = ref([])
const loading: Ref<boolean> = ref(true)
const error: Ref<string | null> = ref(null)
const currentPage: Ref<number> = ref(1)
const pageSize: Ref<number> = ref(6)
const totalPages: Ref<number> = ref(1)

const fetchEntries = async (page: number = 0) => {
    try {
        loading.value = true
        error.value = null

        const params = new URLSearchParams({
            offset: page.toString(),
            limit: pageSize.value.toString(),
            locale_id: '2',
            type_id: '1',
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
    <div class="min-h-screen bg-linear-to-b from-gray-50 to-gray-100">
        <!-- Hero Section -->
        <section class="bg-white border-b border-gray-200">
            <div class="max-w-6xl mx-auto px-4 py-16">
                <div class="text-center">
                    <h1 class="text-5xl font-bold text-gray-900 mb-4">NUR CMS Example Frontend</h1>
                    <p class="text-xl text-gray-600 mb-8">
                        A modern Vue 3 example showcasing how to integrate with the NUR CMS backend API
                    </p>
                    <div class="flex justify-center gap-4">
                        <a
                            href="#entries"
                            class="px-6 py-3 bg-blue-600 text-white rounded-lg hover:bg-blue-700 transition"
                        >
                            Explore Content
                        </a>
                        <a
                            href="https://github.com/nur-cms/nur-cms"
                            target="_blank"
                            class="px-6 py-3 bg-gray-200 text-gray-900 rounded-lg hover:bg-gray-300 transition"
                        >
                            View on GitHub
                        </a>
                    </div>
                </div>
            </div>
        </section>

        <!-- Features Section -->
        <section class="max-w-6xl mx-auto px-4 py-16">
            <h2 class="text-3xl font-bold text-gray-900 mb-12 text-center">Key Features</h2>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
                <div class="bg-white rounded-lg shadow-md p-8 hover:shadow-lg transition">
                    <div class="w-12 h-12 bg-blue-100 rounded-lg flex items-center justify-center mb-4">
                        <svg class="w-6 h-6 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                stroke-width="2"
                                d="M13 10V3L4 14h7v7l9-11h-7z"
                            />
                        </svg>
                    </div>
                    <h3 class="text-xl font-semibold text-gray-900 mb-2">Fast & Efficient</h3>
                    <p class="text-gray-600">
                        Built with Rust and Axum, the backend delivers lightning-fast API responses optimized for
                        performance.
                    </p>
                </div>

                <div class="bg-white rounded-lg shadow-md p-8 hover:shadow-lg transition">
                    <div class="w-12 h-12 bg-green-100 rounded-lg flex items-center justify-center mb-4">
                        <svg class="w-6 h-6 text-green-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                stroke-width="2"
                                d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
                            />
                        </svg>
                    </div>
                    <h3 class="text-xl font-semibold text-gray-900 mb-2">Flexible Content</h3>
                    <p class="text-gray-600">
                        Output content in multiple formats: Markdown, HTML, or AST for complete control over rendering.
                    </p>
                </div>

                <div class="bg-white rounded-lg shadow-md p-8 hover:shadow-lg transition">
                    <div class="w-12 h-12 bg-purple-100 rounded-lg flex items-center justify-center mb-4">
                        <svg class="w-6 h-6 text-purple-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                stroke-width="2"
                                d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4"
                            />
                        </svg>
                    </div>
                    <h3 class="text-xl font-semibold text-gray-900 mb-2">Fully Customizable</h3>
                    <p class="text-gray-600">
                        RESTful API makes it easy to build custom frontends with any framework you prefer.
                    </p>
                </div>
            </div>
        </section>

        <!-- Content Section -->
        <section id="entries" class="max-w-6xl mx-auto px-4 py-16">
            <h2 class="text-3xl font-bold text-gray-900 mb-12 text-center">Latest Articles</h2>

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
            <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8 mb-12">
                <article
                    v-for="entry in entries"
                    :key="entry.id!"
                    class="bg-white rounded-lg shadow-md overflow-hidden hover:shadow-lg transition"
                >
                    <!-- Card Header -->
                    <div class="h-48 bg-linear-to-br from-blue-400 to-blue-600 flex items-center justify-center">
                        <svg
                            class="w-16 h-16 text-white opacity-50"
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                        >
                            <path
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                stroke-width="2"
                                d="M19 20H5a2 2 0 01-2-2V6a2 2 0 012-2h10a2 2 0 012 2v10m-9-12h3m-3 12h3"
                            />
                        </svg>
                    </div>

                    <!-- Card Body -->
                    <div class="p-6">
                        <!-- Category Badge -->
                        <div v-if="entry.slug" class="mb-3">
                            <span
                                class="inline-block px-3 py-1 bg-blue-100 text-blue-800 text-xs font-semibold rounded-full"
                            >
                                {{ entry.slug }}
                            </span>
                        </div>

                        <!-- Title -->
                        <h3 class="text-xl font-bold text-gray-900 mb-2 line-clamp-2">
                            {{ entry.title }}
                        </h3>

                        <!-- Description -->
                        <p class="text-gray-600 mb-4 line-clamp-3">
                            {{ truncateText(entry.description!, 120) }}
                        </p>

                        <!-- Metadata -->
                        <div
                            class="flex items-center justify-between text-sm text-gray-500 mb-4 border-t border-gray-100 pt-4"
                        >
                            <span>
                                {{
                                    entry.created_at
                                        ? new Date(entry.created_at).toLocaleDateString()
                                        : 'Date unavailable'
                                }}
                            </span>
                        </div>

                        <!-- Read More Link -->
                        <a
                            href="#"
                            class="inline-block px-4 py-2 bg-blue-600 text-white rounded hover:bg-blue-700 transition text-sm font-medium"
                        >
                            Read More →
                        </a>
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
        </section>

        <!-- Info Section -->
        <section class="bg-white border-t border-gray-200 mt-16">
            <div class="max-w-6xl mx-auto px-4 py-16">
                <div class="grid grid-cols-1 md:grid-cols-2 gap-12">
                    <div>
                        <h3 class="text-2xl font-bold text-gray-900 mb-4">About This Example</h3>
                        <p class="text-gray-600 mb-4">
                            This frontend demonstrates how to build a modern Vue 3 application with the NUR CMS backend.
                            It showcases essential patterns like API integration, data fetching, pagination, and error
                            handling.
                        </p>
                        <p class="text-gray-600">
                            Use this as a foundation to build your own custom frontends - whether it's a blog,
                            portfolio, documentation site, or any other content-driven application.
                        </p>
                    </div>

                    <div>
                        <h3 class="text-2xl font-bold text-gray-900 mb-4">Getting Started</h3>
                        <ul class="space-y-3">
                            <li class="flex items-start gap-3">
                                <span class="text-blue-600 font-bold">1.</span>
                                <span class="text-gray-600">Clone or fork the NUR CMS repository</span>
                            </li>
                            <li class="flex items-start gap-3">
                                <span class="text-blue-600 font-bold">2.</span>
                                <span class="text-gray-600"
                                    >Install dependencies with
                                    <code class="bg-gray-100 px-2 py-1 rounded">npm install</code></span
                                >
                            </li>
                            <li class="flex items-start gap-3">
                                <span class="text-blue-600 font-bold">3.</span>
                                <span class="text-gray-600"
                                    >Run the backend: <code class="bg-gray-100 px-2 py-1 rounded">cargo run</code></span
                                >
                            </li>
                            <li class="flex items-start gap-3">
                                <span class="text-blue-600 font-bold">4.</span>
                                <span class="text-gray-600"
                                    >Start this frontend:
                                    <code class="bg-gray-100 px-2 py-1 rounded">npm run dev:example</code></span
                                >
                            </li>
                        </ul>
                    </div>
                </div>
            </div>
        </section>
    </div>
</template>
