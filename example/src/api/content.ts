import type { ContentEntrySerializer } from '../../../frontend/src/types/serialized'

export interface ApiListResponse<T> {
    count: number
    next: string | null
    previous: string | null
    results: T[]
}

export interface ArticleQuery {
    limit?: number
    offset?: number
    search?: string
}

const DEFAULT_LOCALE = import.meta.env.VITE_NUR_LOCALE ?? 'en'
const ARTICLE_TYPE = import.meta.env.VITE_NUR_ARTICLE_TYPE ?? 'article'

const ARTICLE_LIST_FIELDS = [
    'id',
    'title',
    'slug',
    'media',
    'created_at',
    'category.name',
    'category.slug',
    'tags',
    'node.ast',
].join(',')

const ARTICLE_DETAIL_FIELDS = [
    'id',
    'title',
    'slug',
    'media',
    'created_at',
    'category.name',
    'category.slug',
    'tags',
    'author.first_name',
    'author.last_name',
    'node.id',
    'node.order_index',
    'node.ast',
    'node.media',
    'node.data',
].join(',')

function buildSearchParams(params: Record<string, string | number | undefined>): URLSearchParams {
    const search = new URLSearchParams()

    for (const [key, value] of Object.entries(params)) {
        if (value !== undefined && value !== '') {
            search.set(key, String(value))
        }
    }

    return search
}

async function fetchJson<T>(url: string): Promise<T> {
    const response = await fetch(url)

    if (!response.ok) {
        throw new Error(`API request failed with ${response.status}`)
    }

    return response.json() as Promise<T>
}

export async function fetchArticles(query: ArticleQuery = {}): Promise<ApiListResponse<ContentEntrySerializer>> {
    const params = buildSearchParams({
        type: ARTICLE_TYPE,
        locale: DEFAULT_LOCALE,
        fields: ARTICLE_LIST_FIELDS,
        limit: query.limit ?? 6,
        offset: query.offset ?? 0,
        ordering: '-created_at',
        blocks_limit: 1,
        character_limit: 240,
        search: query.search,
    })

    return fetchJson<ApiListResponse<ContentEntrySerializer>>(`/api/content/entries?${params}`)
}

export async function fetchArticle(slug: string): Promise<ContentEntrySerializer> {
    const params = buildSearchParams({
        locale: DEFAULT_LOCALE,
        fields: ARTICLE_DETAIL_FIELDS,
    })

    return fetchJson<ContentEntrySerializer>(
        `/api/content/entries/${encodeURIComponent(ARTICLE_TYPE)}/${encodeURIComponent(slug)}?${params}`,
    )
}
