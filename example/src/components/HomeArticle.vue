<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { fetchArticles } from '@/api/content'
import { extractAstText, formatDate, mediaPath, stripHtml } from '@/utils/helper'
import type { MediaSerializer } from '../../../frontend/src/types/serialized'

interface ArticleListNode {
    text?: string | null
    html?: string | null
    ast?: unknown
    blocks?: ArticleListNode[]
}

interface ArticleListEntry {
    id?: number | null
    title?: string | null
    slug?: string | null
    media?: MediaSerializer | null
    category?: {
        name?: string | null
    } | null
    nodes?: ArticleListNode[]
    created_at?: string | null
}

interface ArticleCard {
    key: string
    title: string
    slug: string
    imageSrc: string
    imageAlt: string
    categoryName: string
    createdAt: string
    excerpt: string
}

const entries = ref<ArticleListEntry[]>([])
const loading = ref(true)
const error = ref<string | null>(null)
const search = ref('')
const currentPage = ref(1)
const pageSize = ref(6)
const total = ref(0)

const totalPages = computed(() => Math.max(1, Math.ceil(total.value / pageSize.value)))
const cards = computed<ArticleCard[]>(() =>
    entries.value.map((entry, index) => ({
        key: String(entry.id ?? entry.slug ?? `entry-${index}`),
        title: entry.title ?? 'Untitled',
        slug: entry.slug ?? '',
        imageSrc: mediaPath(entry.media),
        imageAlt: entry.media?.alt ?? entry.title ?? '',
        categoryName: entry.category?.name ?? '',
        createdAt: entry.created_at ? formatDate(entry.created_at) : '',
        excerpt: articleExcerpt(entry) || 'No excerpt available.',
    })),
)

function articleNodes(entry: ArticleListEntry): ArticleListNode[] {
    return (entry.nodes ?? []).flatMap((node) => (Array.isArray(node.blocks) ? node.blocks : [node]))
}

function articleExcerpt(entry: ArticleListEntry, maxLength = 180): string {
    const text = articleNodes(entry)
        .map((node) => node.text ?? node.html ?? extractAstText(node.ast))
        .map((value) => stripHtml(value))
        .filter(Boolean)
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim()

    if (text.length <= maxLength) return text
    return `${text.slice(0, maxLength).replace(/\s+\S*$/, '')}…`
}

async function loadEntries(page = 1) {
    try {
        loading.value = true
        error.value = null
        currentPage.value = page

        const response = await fetchArticles({
            limit: pageSize.value,
            offset: (page - 1) * pageSize.value,
            search: search.value.trim() || undefined,
        })

        entries.value = response.results as ArticleListEntry[]
        total.value = Number(response.count)
    } catch (err) {
        error.value = err instanceof Error ? err.message : 'Failed to load articles'
    } finally {
        loading.value = false
    }
}

function submitSearch() {
    loadEntries(1)
}

onMounted(() => loadEntries())
</script>

<template>
    <div class="w-full max-w-6xl">
        <div class="flex flex-col md:flex-row md:items-end gap-4 mb-10">
            <div class="grow">
                <p class="text-sm uppercase tracking-wide text-primary font-semibold">Published content</p>
                <h2 class="text-3xl font-bold">Latest Articles</h2>
            </div>

            <form class="join" @submit.prevent="submitSearch">
                <label class="input join-item">
                    <i class="bi bi-search opacity-45" />
                    <input v-model="search" type="search" placeholder="Search articles" />
                </label>
                <button class="btn btn-primary join-item" type="submit">Search</button>
            </form>
        </div>

        <div v-if="loading" class="flex justify-center items-center py-16">
            <span class="loading loading-spinner loading-lg" />
        </div>

        <div v-else-if="error" class="alert alert-error">
            <span>{{ error }}</span>
        </div>

        <div
            v-else-if="entries.length === 0"
            class="bg-base-200 border border-base-content/10 rounded-lg p-12 text-center"
        >
            <p class="text-base-content/60 text-lg">No published articles available yet.</p>
        </div>

        <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-8 mb-10">
            <article
                v-for="card in cards"
                :key="card.key"
                class="card bg-base-200 shadow-sm hover:shadow-lg transition"
            >
                <figure v-if="card.imageSrc" class="h-52 bg-base-300">
                    <img :src="card.imageSrc" :alt="card.imageAlt" class="w-full h-full object-cover" />
                </figure>

                <div v-else class="h-52 bg-base-300 flex items-center justify-center text-base-content/30">
                    <i class="bi bi-card-image text-6xl" />
                </div>

                <div class="card-body">
                    <div class="flex flex-wrap gap-2 text-xs text-base-content/60">
                        <span v-if="card.categoryName" class="badge badge-outline">{{ card.categoryName }}</span>
                        <span v-if="card.createdAt">{{ card.createdAt }}</span>
                    </div>

                    <h3 class="card-title">{{ card.title }}</h3>
                    <p class="text-base-content/70">{{ card.excerpt }}</p>

                    <div class="card-actions justify-end mt-4">
                        <RouterLink :to="`/articles/${card.slug}`" class="btn btn-primary">Read More</RouterLink>
                    </div>
                </div>
            </article>
        </div>

        <div v-if="totalPages > 1" class="flex justify-center items-center gap-2">
            <button class="btn btn-sm" :disabled="currentPage <= 1" @click="loadEntries(currentPage - 1)">
                Previous
            </button>
            <button
                v-for="page in totalPages"
                :key="page"
                class="btn btn-sm"
                :class="{ 'btn-primary': page === currentPage }"
                @click="loadEntries(page)"
            >
                {{ page }}
            </button>
            <button class="btn btn-sm" :disabled="currentPage >= totalPages" @click="loadEntries(currentPage + 1)">
                Next
            </button>
        </div>
    </div>
</template>
