<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, RouterLink } from 'vue-router'
import { marked } from 'marked'

import { fetchArticle } from '@/api/content'
import AstRender from '@/components/AstRender.vue'
import { formatDate, mediaPath } from '@/utils/helper'
import type { MediaSerializer } from '../../../frontend/src/types/serialized'

interface ArticleDetailNode {
    id?: number | null
    order_index?: number | null
    text?: string | null
    ast?: unknown
    html?: string | null
    data?: unknown
    media?: MediaSerializer | null
    blocks?: ArticleDetailNode[]
}

interface ArticleTag {
    id?: number | null
    name?: string | null
    slug?: string | null
}

interface ArticleAuthor {
    first_name?: string | null
    last_name?: string | null
}

interface ArticleDetailEntry {
    title?: string | null
    media?: MediaSerializer | null
    category?: {
        name?: string | null
    } | null
    tags?: ArticleTag[]
    authors?: ArticleAuthor[]
    nodes?: ArticleDetailNode[]
    created_at?: string | null
}

const route = useRoute()
const entry = ref<ArticleDetailEntry | null>(null)
const loading = ref(true)
const error = ref<string | null>(null)

const nodes = computed(() => flattenNodes(entry.value))
const authors = computed(() =>
    entry.value?.authors
        ?.map((author) => `${author.first_name ?? ''} ${author.last_name ?? ''}`.trim())
        .filter(Boolean),
)

function flattenNodes(article?: ArticleDetailEntry | null): ArticleDetailNode[] {
    return (article?.nodes ?? []).flatMap((node) => (Array.isArray(node.blocks) ? node.blocks : [node]))
}

async function loadArticle() {
    try {
        loading.value = true
        error.value = null
        entry.value = (await fetchArticle(String(route.params.slug))) as ArticleDetailEntry
    } catch (err) {
        error.value = err instanceof Error ? err.message : 'Failed to load article'
    } finally {
        loading.value = false
    }
}

function markdownToHtml(value: string): string {
    return marked.parse(value, { async: false }) as string
}

function nodeHasRenderableContent(node: ArticleDetailNode): boolean {
    return Boolean(node.ast || node.html || node.text || node.media || node.data)
}

function nodeKey(node: ArticleDetailNode, index: number): string {
    return String(node.id ?? node.order_index ?? `node-${index}`)
}

function tagKey(tag: ArticleTag, index: number): string {
    return String(tag.slug ?? tag.name ?? tag.id ?? `tag-${index}`)
}

onMounted(loadArticle)
</script>

<template>
    <div class="max-w-4xl mx-auto px-4 py-10">
        <RouterLink to="/" class="btn btn-sm btn-ghost mb-6">
            <i class="bi bi-arrow-left" />
            Back to articles
        </RouterLink>

        <div v-if="loading" class="flex justify-center py-20">
            <span class="loading loading-spinner loading-lg" />
        </div>

        <div v-else-if="error" class="alert alert-error">
            <span>{{ error }}</span>
        </div>

        <article v-else-if="entry" class="space-y-8">
            <header class="space-y-4">
                <div class="flex flex-wrap gap-2 text-sm text-base-content/60">
                    <span v-if="entry.category?.name" class="badge badge-outline">{{ entry.category.name }}</span>
                    <span v-if="entry.created_at">{{ formatDate(entry.created_at) }}</span>
                    <span v-if="authors?.length">by {{ authors.join(', ') }}</span>
                </div>

                <h1 class="text-4xl md:text-5xl font-bold leading-tight">{{ entry.title }}</h1>

                <div v-if="entry.tags?.length" class="flex flex-wrap gap-2">
                    <span v-for="(tag, index) in entry.tags" :key="tagKey(tag, index)" class="badge badge-primary">
                        {{ tag.name }}
                    </span>
                </div>
            </header>

            <img
                v-if="entry.media"
                :src="mediaPath(entry.media, 1280)"
                :alt="entry.media.alt ?? entry.title ?? ''"
                class="w-full max-h-[32rem] object-cover rounded-xl"
            />

            <div class="prose prose-lg max-w-none">
                <template v-for="(node, index) in nodes" :key="nodeKey(node, index)">
                    <AstRender v-if="node.ast" :content="node.ast" />
                    <div v-else-if="node.html" v-html="node.html" />
                    <div v-else-if="node.text" v-html="markdownToHtml(node.text)" />
                    <img
                        v-else-if="node.media"
                        :src="mediaPath(node.media, 1280)"
                        :alt="node.media.alt ?? ''"
                        class="rounded-lg"
                    />
                    <pre v-else-if="node.data"><code>{{ JSON.stringify(node.data, null, 2) }}</code></pre>
                    <template v-else-if="nodeHasRenderableContent(node)" />
                </template>
            </div>
        </article>
    </div>
</template>
