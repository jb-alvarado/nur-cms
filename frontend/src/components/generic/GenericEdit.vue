<script setup lang="ts">
import { ref, computed, inject, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import dayjs from 'dayjs'
import { useRoute, useRouter, RouterLink } from 'vue-router'
import { useClipboard } from '@vueuse/core'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import Multiselect from 'vue-multiselect'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { authFetch } from '@/composables/authFetch'
import { closeDropdown, mediaPath } from '@/utils/helper'
import { slugify } from '@/utils/slugify.js'
import { genericEditConfigKey, type GenericEditField, type GenericEditStatus } from '@/types/generic-edit'

import GenericBlock from './GenericBlock.vue'
import GenericModal from './GenericModal.vue'
import BlockModal from '@/components/BlockModal.vue'
import MarkdownPreview from '@/components/MarkdownPreview.vue'
import MediaBrowser from '@/components/media/MediaBrowser.vue'
import TextEditor from '@/components/TextEditor.vue'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const store = useIndex()
const genericEditConfig = inject(genericEditConfigKey, {})

const rootPath = route.path.replace(/\/[0-9/]+$/g, '')
const routeType = String(route.params.type ?? store.routeType)
const contentId = Number(route.params.id ?? 0)
const groupID = Number(route.params.group_id ?? 0)
const defaultStatus = genericEditConfig[routeType]?.defaultStatus ?? genericEditConfig['*']?.defaultStatus ?? 'draft'

const deleteModal = ref()
const mediaModal = ref()
const blockModal = ref()
const editorEndRef = ref<HTMLElement | null>(null)
const mediaTarget = ref<{ type: 'main' | 'node' | 'block'; nodeIndex?: number; blockIndex?: number }>({
    type: 'main',
})
const mediaTypeFilter = computed(() => (mediaTarget.value.type === 'main' ? [] : ['image']))
const editorNodeKeys = new WeakMap<object, number>()
let nextEditorNodeKey = 1
const dropValueRaw = ref('')
const dropValue = computed({
    get: () => dropValueRaw.value,
    set: (value: string) => {
        dropValueRaw.value = value

        if (!value) return

        try {
            const json = JSON.parse(value)

            if (Array.isArray(json)) {
                const nIndex = currentNodeIndex.value
                for (const obj of json) {
                    addDataNode({ name: null, media: null, data: obj })
                    currentNodeIndex.value = nIndex
                }

                currentNodeIndex.value = -1
            } else {
                addDataNode({ name: null, media: null, data: json })
            }

            dropValueRaw.value = ''
        } catch {
            store.msgAlert('error', 'No valid json data!')
        }
    },
})

function handleEmptyStatePaste(event: ClipboardEvent) {
    const pasted = event.clipboardData?.getData('text')
    if (!pasted) return

    event.preventDefault()

    function removeIds(obj: any): any {
        if (Array.isArray(obj)) {
            return obj.map(removeIds)
        } else if (obj && typeof obj === 'object') {
            const newObj: any = {}
            for (const key in obj) {
                if (key === 'id') continue
                newObj[key] = removeIds(obj[key])
            }
            return newObj
        }
        return obj
    }

    const json = JSON.parse(pasted)
    content.value.nodes = removeIds(json)
}

const content = ref({
    id: 0,
    group_id: groupID,
    type: '',
    title: '',
    slug: '',
    nodes: [],
    status: defaultStatus,
    locale_id: 0,
    group_members: [],
    check: false,
    meta: {},
} as Content)
const contentOriginal = ref(cloneDeep(content))

const { copy, copied, isSupported } = useClipboard()

function copyStructure() {
    copy(JSON.stringify(content.value.nodes))
}

contentOriginal.value.group_id = 0
const media = ref<Media | null>(null)
const categories = ref<Category[]>([])
const tags = ref<Tag[]>([])
const locales = ref<Locale[]>([])
type NodeTemplateSchema = {
    id: number
    schema: Array<{ key: string; label?: string | null; kind?: 'string' | 'text' | 'boolean' | 'number' | 'json' }>
}
const nodeTemplates = ref<NodeTemplateSchema[]>([])
const needsSave = computed(() => !isEqual(content.value, contentOriginal.value))
const status: GenericEditStatus[] = ['draft', 'published', 'archived']
const currentNodeIndex = ref(-1)
const templateCount = ref(0)
const currentContentType = computed(() => store.types.find((item) => item.slug === store.routeType))
const disabledFields = computed(() => {
    return new Set<GenericEditField>([
        ...(genericEditConfig['*']?.disabledFields ?? []),
        ...(genericEditConfig[routeType]?.disabledFields ?? []),
    ])
})
const isFieldEnabled = (field: GenericEditField) => !disabledFields.value.has(field)
const showMetaFields = computed(() => {
    const hasEnabledMetaField = isFieldEnabled('start_time') || isFieldEnabled('end_time')
    const usesMeta =
        content.value.meta?.start_time || content.value.meta?.end_time || currentContentType.value?.use_meta

    return Boolean(hasEnabledMetaField && usesMeta)
})

const authorsFormatted = computed(() =>
    store.authors.map((a) => ({
        ...a,
        displayName: `${a.first_name} ${a.last_name ?? ''}`.trim(),
    })),
)

const selectedAuthorsFormatted = computed({
    get: () =>
        content.value.authors?.map((a) => ({
            ...a,
            displayName: `${a.first_name} ${a.last_name ?? ''}`.trim(),
        })) ?? [],
    set: (value) => {
        content.value.authors = value.map((v: any) => {
            const r = { ...v }
            delete r.displayName
            return r
        })
    },
})

const selectedCategory = computed({
    get: () => {
        if (!content.value.category_id) return null
        return categories.value.find((c) => c.id === content.value.category_id) ?? content.value.category ?? null
    },
    set: (value: Category | null) => {
        content.value.category_id = value?.id ?? null
    },
})

function autoSelectSingleStoreLocale() {
    if (contentId === 0 && !content.value.locale_id && store.locales.length === 1) {
        content.value.locale_id = store.locales[0]?.id ?? 0
    }
}

autoSelectSingleStoreLocale()

if (contentId > 0) {
    selectContent()
} else if (groupID > 0) {
    authFetch<RespondObj>(
        `/api/content/entries?type_slug=${store.routeType}&group_id=${groupID}&fields=locale_id,group_members&output_type=markdown`,
    )
        .then((response: RespondObj) => {
            addTextNode()
            const groupMemberLocales = new Set(
                response.results.flatMap(
                    (result: RespondObj) =>
                        result.group_members?.map((member: GroupMember) => member.locale_code) ?? [result.locale_code],
                ),
            )
            locales.value = store.locales.filter((locale) => !groupMemberLocales.has(locale.code))
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
} else {
    addTextNode()

    setTimeout(() => {
        locales.value = store.locales
        autoSelectSingleStoreLocale()
    }, 1000)
}

if (isFieldEnabled('category')) {
    selectCategories()
}
if (isFieldEnabled('tags')) {
    selectTags()
}
selectNodeTemplates()

function selectContent() {
    authFetch<RespondObj>(`/api/content/entries?type_slug=${store.routeType}&id=${contentId}&output_type=markdown`)
        .then((response: RespondObj) => {
            if (response.results.length > 0) {
                content.value = response.results[0]

                if (content.value.meta) {
                    if (content.value.meta.start_time) {
                        content.value.meta.start_time = dayjs(content.value.meta.start_time).format('YYYY-MM-DD HH:mm')
                    }
                    if (content.value.meta.end_time) {
                        content.value.meta.end_time = dayjs(content.value.meta.end_time).format('YYYY-MM-DD HH:mm')
                    }
                } else {
                    content.value.meta = {}
                }

                if (!content.value.nodes) {
                    content.value.nodes = []
                    addTextNode()
                }

                contentOriginal.value = cloneDeep(content.value)

                locales.value = store.locales.filter((locale) => {
                    const isCurrentLocale = locale.id === content.value.locale_id
                    const hasGroupMember = content.value.group_members?.some(
                        (member) => member.locale_code === locale.code,
                    )
                    return isCurrentLocale || hasGroupMember
                })

                if (content.value.media_id) {
                    selectMedia()
                }
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectMedia() {
    await authFetch<RespondObj>(`/api/media?id=${content.value.media_id}`)
        .then((response: RespondObj) => {
            media.value = response.results[0]
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectCategories() {
    await fetch(`/api/content/categories?fields=id,group_id,locale_id,name,slug`)
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
        .then((response: RespondObj) => {
            const byGroup = new Map<number, Category[]>()
            const picked: Category[] = []

            for (const c of response.results as Category[]) {
                const g = c.group_id ?? 0
                if (!byGroup.has(g)) byGroup.set(g, [])
                byGroup.get(g)!.push(c)
            }

            for (const groupCats of byGroup.values()) {
                const match = groupCats.find((c) => c.locale_id === content.value.locale_id)
                if (match) {
                    picked.push(match)
                } else if (groupCats.length > 0) {
                    picked.push(groupCats[0]!)
                }
            }

            categories.value = picked
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectTags() {
    await fetch(`/api/content/tags?fields=id,name,slug&limit=200`)
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
        .then((response: RespondObj) => {
            tags.value = response.results
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectNodeTemplates() {
    try {
        const response = await authFetch<RespondObj>('/api/content/node/templates?ordering=id')
        nodeTemplates.value = response.results.map((template: any) => ({
            id: Number(template.id),
            schema: template.schema ?? [],
        }))
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}

function nodeTemplateSchema(node: ContentNodeSerializer) {
    const templateId = node.template_id
    return nodeTemplates.value.find((template) => template.id === templateId)?.schema ?? []
}

function updateSlug() {
    if (content.value.title) {
        content.value.slug = slugify(content.value.title)
    }
}

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

const openMediaBrowser = () => {
    mediaTarget.value = { type: 'main' }
    mediaModal.value.showModal()
}

const openNodeMediaBrowser = (nodeIndex: number) => {
    mediaTarget.value = { type: 'node', nodeIndex }
    mediaModal.value.showModal()
}

const openBlockMediaBrowser = (nodeIndex: number, blockIndex: number) => {
    mediaTarget.value = { type: 'block', nodeIndex, blockIndex }
    mediaModal.value.showModal()
}

const openBlockModal = (index: number) => {
    currentNodeIndex.value = index
    blockModal.value.showModal()
}

async function scrollEditorToEnd() {
    await nextTick()
    editorEndRef.value?.scrollIntoView({ behavior: 'smooth', block: 'end' })
}

function addTextNode() {
    content.value.nodes?.push({
        order_index: (content.value.nodes?.length ?? 0) + 1,
        text: '',
    })

    scrollEditorToEnd()
}

function addBlocksNode() {
    content.value.nodes?.push({
        blocks: [],
    })

    scrollEditorToEnd()
}

function requestedPosition(event: Event, fallback: number, maximum: number): number {
    const input = event.currentTarget as HTMLInputElement
    const value = Number.parseInt(input.value, 10)

    if (!Number.isFinite(value)) {
        input.value = String(fallback)
        return fallback
    }

    const position = Math.min(Math.max(value, 1), maximum)
    input.value = String(position)

    return position
}

function editorNodeKey(node: object): number {
    const existingKey = editorNodeKeys.get(node)
    if (existingKey !== undefined) return existingKey

    const key = nextEditorNodeKey
    nextEditorNodeKey += 1
    editorNodeKeys.set(node, key)

    return key
}

function moveItem<T>(items: T[], fromIndex: number, toPosition: number) {
    const toIndex = toPosition - 1
    if (fromIndex === toIndex) return

    const [item] = items.splice(fromIndex, 1)
    if (item !== undefined) items.splice(toIndex, 0, item)
}

function normalizeNodeOrderIndexes() {
    let orderIndex = 1

    for (const node of content.value.nodes ?? []) {
        if ('blocks' in node && Array.isArray(node.blocks)) {
            for (const block of node.blocks) {
                block.order_index = orderIndex
                orderIndex += 1
            }
        } else {
            const simpleNode = node as ContentNodeSerializer
            simpleNode.order_index = orderIndex
            orderIndex += 1
        }
    }
}

function reorderNode(index: number, event: Event) {
    const nodes = content.value.nodes
    if (!nodes?.length) return

    const position = requestedPosition(event, index + 1, nodes.length)
    moveItem(nodes, index, position)
    normalizeNodeOrderIndexes()
}

function reorderBlock(nodeIndex: number, blockIndex: number, event: Event) {
    const node = content.value.nodes?.[nodeIndex] as { blocks: Array<ContentNodeSerializer> } | undefined
    if (!node?.blocks.length) return

    const position = requestedPosition(event, blockIndex + 1, node.blocks.length)
    moveItem(node.blocks, blockIndex, position)
    normalizeNodeOrderIndexes()
}

function addDataNode(item: {
    name: null | string
    media: null | Media
    data: Record<string, any>
    template_id?: number
}) {
    if (!content.value.nodes) {
        content.value.nodes = []
    }

    if (currentNodeIndex.value > -1 && content.value.nodes && content.value.nodes[currentNodeIndex.value]) {
        const node = content.value.nodes[currentNodeIndex.value] as { blocks: Array<ContentNodeSerializer> }
        if (!node.blocks) {
            node.blocks = []
        }

        node.blocks.push({
            media_id: item.media?.id ?? null,
            name: item.name,
            data: item.data,
            template_id: item.template_id,
            media: item.media,
            order_index: (node.blocks?.length ?? 0) + 1,
        } as any)
    } else {
        content.value.nodes.push({
            media_id: item.media?.id ?? null,
            name: item.name,
            data: item.data,
            template_id: item.template_id,
            media: item.media,
            order_index: (content.value.nodes?.length ?? 0) + 1,
        } as any)
    }

    currentNodeIndex.value = -1
    scrollEditorToEnd()
}

function deleteNode(index: number, blockIndex: number | null = null) {
    if (content.value.nodes) {
        if (blockIndex !== null) {
            const node = content.value.nodes[index] as { blocks: Array<ContentNodeSerializer> }
            node.blocks.splice(blockIndex, 1)
        } else {
            content.value.nodes.splice(index, 1)
        }

        normalizeNodeOrderIndexes()
    }
}

function memberLink(code: string): string {
    const member = content.value.group_members?.find((member) => member.locale_code === code)

    return `${rootPath}/${member?.id ?? content.value.id}`
}

async function save() {
    // Build payload with only changed fields
    const payload: Record<string, any> = Object.fromEntries(
        Object.entries(content.value as Record<string, any>).filter(([key, value]) => {
            return !isEqual(value, (contentOriginal.value as Record<string, any>)[key])
        }),
    )

    // New entries must send their configured initial status because it is unchanged
    // relative to contentOriginal and would otherwise be omitted from the payload.
    if (contentId === 0) {
        payload.status = content.value.status
    }

    // Calculate tag changes
    const originalTagIds = new Set(contentOriginal.value.tags?.map((t) => t.id) ?? [])
    const currentTagIds = new Set(content.value.tags?.map((t) => t.id) ?? [])
    const deletedTags = contentOriginal.value.tags?.filter((t) => !currentTagIds.has(t.id)) ?? []
    const newTags = content.value.tags?.filter((t) => !originalTagIds.has(t.id)) ?? []

    // Calculate author changes
    const originalAuthorIds = new Set(contentOriginal.value.authors?.map((a) => a.id) ?? [])
    const currentAuthorIds = new Set(content.value.authors?.map((a) => a.id) ?? [])
    const deletedAuthors = contentOriginal.value.authors?.filter((a) => !currentAuthorIds.has(a.id)) ?? []
    const newAuthors = content.value.authors?.filter((a) => !originalAuthorIds.has(a.id)) ?? []

    // Remove non-saveable fields from payload
    delete payload.authors
    delete payload.category
    delete payload.media
    delete payload.tags

    // Early validation
    if (
        Object.keys(payload).length === 0 &&
        deletedTags.length === 0 &&
        newTags.length === 0 &&
        deletedAuthors.length === 0 &&
        newAuthors.length === 0
    ) {
        store.msgAlert('warning', t('common.noChanges'))
        return
    }

    if (contentId === 0 && !payload.locale_id) {
        store.msgAlert('warning', t('common.selectLanguage'))
        return
    }

    // Convert meta datetime-local format to RFC3339 (after validation)
    if (payload.meta) {
        if (payload.meta.start_time) {
            // Convert from datetime-local (YYYY-MM-DDTHH:mm) to RFC3339
            const date = new Date(payload.meta.start_time)
            payload.meta.start_time = date.toISOString()
        }
        if (payload.meta.end_time) {
            const date = new Date(payload.meta.end_time)
            payload.meta.end_time = date.toISOString()
        }
    }

    // Remove media objects from nodes before save (keep media_id)
    if (payload.nodes) {
        payload.nodes = payload.nodes.map((node: any) => {
            if (node && typeof node === 'object') {
                if ('blocks' in node && Array.isArray(node.blocks)) {
                    node.blocks = node.blocks.map((block: any) => {
                        if (block && typeof block === 'object') {
                            if (!block.media_id && block.media?.id) {
                                block.media_id = block.media.id
                            }
                            delete block.media
                        }
                        return block
                    })
                } else {
                    if (!node.media_id && node.media?.id) {
                        node.media_id = node.media.id
                    }
                    delete node.media
                }
            }
            return node
        })
    }

    let saved = false

    try {
        // Handle tag and author changes for existing entries
        if (contentId > 0) {
            await Promise.all([
                ...deletedTags.map((tag) => deleteEntryTag(contentId, tag.id!)),
                ...newTags.map((tag) => insertEntryTag(contentId, tag.id!)),
                ...deletedAuthors.map((author) => deleteEntryAuthor(contentId, author.id!)),
                ...newAuthors.map((author) => insertEntryAuthor(contentId, author.id!)),
            ])
            saved = deletedTags.length + newTags.length + deletedAuthors.length + newAuthors.length > 0
        } else {
            payload.type_id = store.types.find((t) => t.slug === store.routeType)?.id
        }

        // Save entry if there are payload changes
        if (Object.keys(payload).length > 0) {
            const newId = await authFetch<number>(`/api/content/entries${contentId > 0 ? `/${contentId}` : ''}`, {
                method: contentId > 0 ? 'PUT' : 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify(payload),
            })
            saved = true

            // Handle new entry creation
            if (contentId === 0) {
                await Promise.all([
                    ...newTags.map((tag) => insertEntryTag(newId, tag.id!)),
                    ...newAuthors.map((author) => insertEntryAuthor(newId, author.id!)),
                ])

                store.msgAlert('success', t('common.saveSuccess'))
                router.push(rootPath)
                return
            }
        }

        if (saved) {
            store.msgAlert('success', t('common.saveSuccess'))
        }
        selectContent()
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}

function deleteContent() {
    if (contentId > 0) {
        authFetch(`/api/content/entries/${contentId}`, {
            method: 'DELETE',
        })
            .then(() => {
                store.msgAlert('success', t('common.deleteSuccess', { name: content.value.title ?? content.value.id }))

                router.push(rootPath)
            })
            .catch((e) => {
                store.msgAlert('error', e)
            })
    }
}

function addMedia(m: Media) {
    if (mediaTarget.value.type === 'node') {
        const node = content.value.nodes?.[mediaTarget.value.nodeIndex ?? -1] as ContentNodeSerializer | undefined
        if (node) {
            node.media_id = m.id
            node.media = m
        }
    } else if (mediaTarget.value.type === 'block') {
        const node = content.value.nodes?.[mediaTarget.value.nodeIndex ?? -1] as
            { blocks: Array<ContentNodeSerializer> } | undefined
        const block = node?.blocks?.[mediaTarget.value.blockIndex ?? -1]

        if (block) {
            block.media_id = m.id
            block.media = m
        }
    } else {
        content.value.media_id = m.id
        media.value = m
    }

    mediaTarget.value = { type: 'main' }

    mediaModal.value.close()
}

function removeMedia() {
    content.value.media_id = null
    content.value.media = null
    media.value = null
}

function removeCategory() {
    content.value.category_id = null
}

function insertTag(tag: string) {
    const payload: Tag = {
        name: tag,
        slug: slugify(tag),
    }

    authFetch<number>('/api/content/tags', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (id) => {
            await selectTags()
            payload.id = id

            content.value.tags?.push(payload)
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function deleteEntryTag(entry_id: number, tag_id: number) {
    await authFetch(`/api/content/entries/${entry_id}/tag/${tag_id}`, {
        method: 'DELETE',
    })
}

async function insertEntryTag(entry: number, tag: number) {
    const payload = {
        entry_id: entry,
        tag_id: tag,
    }

    await authFetch('/api/content/entries/tag', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
}

async function deleteEntryAuthor(entry_id: number, author_id: number) {
    await authFetch(`/api/content/entries/${entry_id}/author/${author_id}`, {
        method: 'DELETE',
    })
}

async function insertEntryAuthor(entry: number, author: number) {
    const payload = {
        entry_id: entry,
        author_id: author,
    }

    await authFetch('/api/content/entries/author', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
}
</script>

<template>
    <div class="flex flex-col h-full">
        <div class="flex">
            <h1 class="grow text-xl lg:text-2xl lin">{{ content?.title ?? '' }}</h1>
            <button class="btn btn-sm text-base" @click="router.back()">
                <i class="bi bi-chevron-left" />
            </button>
        </div>

        <div class="flex md:gap-2 h-[calc(100%-32px)]">
            <div class="flex flex-col h-full">
                <!-- Form + Editor Container -->
                <div
                    v-if="content"
                    class="flex flex-col flex-1 w-full md:w-auto md:max-w-5xl bg-base-300 px-4 pt-1 mt-4 rounded"
                    :class="templateCount > 0 ? 'pb-2' : 'pb-4'"
                >
                    <!-- Form inputs -->
                    <div class="flex flex-wrap-reverse gap-4">
                        <div
                            v-if="isFieldEnabled('title') || isFieldEnabled('slug')"
                            class="grow flex flex-col md:flex-row gap-2"
                        >
                            <fieldset v-if="isFieldEnabled('title')" class="fieldset w-full md:w-64">
                                <legend class="fieldset-legend">{{ $t('table.title') }}</legend>
                                <input
                                    v-model="content.title"
                                    type="text"
                                    class="input w-full"
                                    name="title"
                                    :placeholder="$t('table.title')"
                                    @input="updateSlug()"
                                />
                            </fieldset>

                            <fieldset v-if="isFieldEnabled('slug')" class="fieldset w-full md:w-64">
                                <legend class="fieldset-legend">{{ $t('article.slug') }}</legend>
                                <input
                                    v-model="content.slug"
                                    type="text"
                                    class="input w-full"
                                    :placeholder="$t('article.slug')"
                                />
                            </fieldset>
                        </div>

                        <div class="mt-3 md:mt-8 w-full lg:w-auto flex gap-2 flex-wrap md:flex-none">
                            <div class="join">
                                <template v-if="store.locales.length > 1">
                                    <details v-if="content.id === 0" class="dropdown">
                                        <summary class="btn join-item" @blur="closeDropdown">
                                            {{
                                                store.locales.find((l) => l.id === content.locale_id)?.name ||
                                                $t('common.language')
                                            }}
                                        </summary>
                                        <ul
                                            class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-1 shadow-sm"
                                        >
                                            <li v-for="l in locales" :key="l.id">
                                                <a @click="content.locale_id = l.id">{{ l.name }}</a>
                                            </li>
                                        </ul>
                                    </details>

                                    <details v-if="(content.id ?? 0) > 0" class="dropdown">
                                        <summary class="btn join-item" @blur="closeDropdown">
                                            {{ store.locales.find((l) => l.id === content.locale_id)?.name }}
                                        </summary>
                                        <ul
                                            class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-1 shadow-sm"
                                        >
                                            <li v-for="l in locales" :key="l.id">
                                                <RouterLink :to="memberLink(l.code!)">{{ l.name }}</RouterLink>
                                            </li>
                                        </ul>
                                    </details>

                                    <RouterLink
                                        :to="`${rootPath}/0/${content.group_id}`"
                                        class="btn join-item px-2"
                                        :title="$t('common.addLanguage')"
                                    >
                                        <i class="bi bi-plus-lg"></i>
                                    </RouterLink>
                                </template>

                                <details v-if="isFieldEnabled('status')" class="dropdown">
                                    <summary
                                        class="btn join-item"
                                        :class="{
                                            'text-success': content.status === 'published',
                                            'text-base-content/50': content.status === 'archived',
                                        }"
                                        @blur="closeDropdown"
                                    >
                                        {{ content.status }}
                                    </summary>
                                    <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-24 p-1 shadow-sm">
                                        <li
                                            v-for="s in status"
                                            :key="s"
                                            :class="{
                                                'text-base-content/50': content.status !== s,
                                            }"
                                        >
                                            <a @click="content.status = s">{{ s }}</a>
                                        </li>
                                    </ul>
                                </details>
                            </div>

                            <div class="join xs:ms-auto">
                                <button
                                    v-if="isFieldEnabled('delete')"
                                    class="btn text-warning join-item"
                                    @click="openDeleteModal()"
                                >
                                    {{ $t('common.delete') }}
                                </button>
                                <button class="btn join-item" :class="{ 'btn-primary': needsSave }" @click="save()">
                                    {{ $t('user.save') }}
                                </button>
                            </div>
                        </div>
                    </div>

                    <div class="flex flex-col md:flex-row gap-2 mt-1">
                        <div class="w-full md:w-64 flex gap-1">
                            <div
                                class="bg-checker w-full md:w-53 aspect-video flex justify-center items-center border border-base-content/20"
                            >
                                <img
                                    v-if="media"
                                    :src="mediaPath(media)"
                                    :alt="media?.alt ?? $t('button.media')"
                                    class="w-full h-full object-contain"
                                />
                            </div>
                            <div class="join join-vertical">
                                <button class="btn p-2 join-item" @click="openMediaBrowser()">
                                    <i class="bi bi-card-image text-xl"></i>
                                </button>
                                <button class="btn p-2 join-item" @click="removeMedia()">
                                    <i class="bi bi-trash text-xl"></i>
                                </button>
                            </div>
                        </div>

                        <div class="grow flex flex-col gap-2">
                            <div class="flex flex-wrap w-full gap-2">
                                <fieldset
                                    v-if="isFieldEnabled('author')"
                                    class="fieldset py-0 grow w-full md:w-auto md:min-w-64"
                                >
                                    <legend class="fieldset-legend pt-0">{{ $t('article.authors') }}</legend>
                                    <Multiselect
                                        v-model="selectedAuthorsFormatted"
                                        track-by="id"
                                        label="displayName"
                                        :placeholder="$t('article.selectAuthor')"
                                        :options="authorsFormatted"
                                        aria-label="pick a author"
                                        :multiple="true"
                                    >
                                    </Multiselect>
                                </fieldset>
                                <fieldset
                                    v-if="isFieldEnabled('category')"
                                    class="fieldset py-0 grow w-full md:w-auto md:min-w-46"
                                >
                                    <legend class="fieldset-legend pt-0">{{ $t('article.category') }}</legend>
                                    <Multiselect
                                        v-model="selectedCategory"
                                        track-by="id"
                                        label="name"
                                        :placeholder="$t('article.selectCategory')"
                                        :options="categories"
                                        aria-label="pick a category"
                                        @remove="removeCategory()"
                                    >
                                    </Multiselect>
                                </fieldset>
                            </div>

                            <fieldset v-if="isFieldEnabled('tags')" class="fieldset py-0 w-full md:w-auto">
                                <legend class="fieldset-legend pt-0">{{ $t('article.tags') }}</legend>
                                <Multiselect
                                    v-model="content.tags"
                                    track-by="id"
                                    label="name"
                                    :placeholder="$t('article.selectTag')"
                                    :options="tags"
                                    aria-label="pick a tag"
                                    :multiple="true"
                                    :taggable="true"
                                    @tag="insertTag"
                                >
                                </Multiselect>
                            </fieldset>

                            <div v-if="showMetaFields" class="flex flex-wrap gap-2">
                                <fieldset v-if="isFieldEnabled('start_time')" class="flex-1 fieldset py-0 min-w-50">
                                    <legend class="fieldset-legend pt-0">{{ $t('common.start') }}</legend>
                                    <input
                                        v-model="content.meta!.start_time"
                                        type="datetime-local"
                                        class="input w-full"
                                    />
                                </fieldset>
                                <fieldset v-if="isFieldEnabled('end_time')" class="flex-1 fieldset py-0 min-w-50">
                                    <legend class="fieldset-legend pt-0">{{ $t('common.end') }}</legend>
                                    <input
                                        v-model="content.meta!.end_time"
                                        type="datetime-local"
                                        class="input w-full"
                                    />
                                </fieldset>
                            </div>
                        </div>
                    </div>

                    <!-- Nodes -->

                    <template v-if="content.nodes && content.nodes.length > 0">
                        <template v-for="(node, i) in content.nodes" :key="editorNodeKey(node)">
                            <TextEditor
                                v-if="!('blocks' in node) && !('data' in node)"
                                v-model="node.text"
                                class="min-h-60"
                                :remove-node="templateCount > 0 ? () => deleteNode(i) : null"
                                :order-position="i + 1"
                                :order-maximum="content.nodes.length"
                                @reorder="reorderNode(i, $event)"
                            />
                            <div
                                v-else-if="'data' in node"
                                class="bg-base-200 rounded mt-2 ps-1 py-1 flex items-center gap-1 border border-base-content/30"
                            >
                                <div class="w-10">
                                    <img
                                        v-if="node.media"
                                        :src="mediaPath(node.media!)"
                                        :alt="node.media?.alt ?? undefined"
                                        class="object-cover w-10 h-10 cursor-pointer"
                                        @click="openNodeMediaBrowser(i)"
                                    />
                                    <div
                                        v-else
                                        class="bg-base-content/30 w-full h-10 cursor-pointer"
                                        @click="openNodeMediaBrowser(i)"
                                    ></div>
                                </div>
                                <GenericBlock
                                    v-model:block="node.data"
                                    :schema="nodeTemplateSchema(node)"
                                    class="grow"
                                />
                                <div class="join">
                                    <input
                                        :value="i + 1"
                                        type="number"
                                        min="1"
                                        :max="content.nodes.length"
                                        step="1"
                                        class="input w-15 join-item"
                                        :title="$t('table.order')"
                                        :aria-label="$t('table.order')"
                                        @change="reorderNode(i, $event)"
                                    />
                                    <button class="btn leading-0 w-10 join-item" @click="deleteNode(i)">
                                        <i class="bi bi-x-lg"></i>
                                    </button>
                                </div>
                            </div>
                            <div v-else-if="'blocks' in node" class="mt-4 border border-base-content/30 rounded">
                                <div class="flex items-center">
                                    <h3 class="text-xl ps-1">{{ $t('common.blocks') }}</h3>
                                    <div class="grow flex justify-end items-center gap-2">
                                        <div class="join">
                                            <button
                                                class="btn leading-0 w-10 join-item"
                                                :title="$t('common.newBlock')"
                                                @click="openBlockModal(i)"
                                            >
                                                <i class="bi bi-plus-lg scale-130"></i>
                                            </button>
                                            <input
                                                :value="i + 1"
                                                type="number"
                                                min="1"
                                                :max="content.nodes.length"
                                                step="1"
                                                class="input w-15 join-item"
                                                :title="$t('table.order')"
                                                :aria-label="$t('table.order')"
                                                @change="reorderNode(i, $event)"
                                            />
                                            <button
                                                class="btn leading-0 w-10 join-item"
                                                :title="$t('common.removeBlock')"
                                                @click="deleteNode(i)"
                                            >
                                                <i class="bi bi-x-lg"></i>
                                            </button>
                                        </div>
                                    </div>
                                </div>
                                <div v-if="node.blocks.length === 0" class="bg-base-200 w-full min-h-6 mt-2">
                                    <input
                                        v-model="dropValue"
                                        class="w-full h-full focus:outline-0 text-base-content/10 cursor-default"
                                        @focus="currentNodeIndex = i"
                                    />
                                </div>
                                <div v-else class="rounded flex flex-col gap-2 mt-2 border border-base-content/30">
                                    <div
                                        v-for="(block, bi) in node.blocks"
                                        :key="block.id ?? bi"
                                        class="bg-base-200 rounded ps-1 py-1 flex gap-1"
                                    >
                                        <div class="w-10">
                                            <img
                                                v-if="block.media"
                                                :src="mediaPath(block.media!)"
                                                :alt="block.media?.alt ?? undefined"
                                                class="object-cover w-10 h-10 cursor-pointer"
                                                @click="openBlockMediaBrowser(i, bi)"
                                            />
                                            <div
                                                v-else
                                                class="bg-base-content/30 w-full h-10 cursor-pointer"
                                                @click="openBlockMediaBrowser(i, bi)"
                                            ></div>
                                        </div>
                                        <GenericBlock
                                            v-model:block="block.data"
                                            :schema="nodeTemplateSchema(block)"
                                            class="grow"
                                        />
                                        <div class="join">
                                            <input
                                                :value="bi + 1"
                                                type="number"
                                                min="1"
                                                :max="node.blocks.length"
                                                step="1"
                                                class="input w-15 join-item"
                                                :title="$t('table.order')"
                                                :aria-label="$t('table.order')"
                                                @change="reorderBlock(i, bi, $event)"
                                            />
                                            <button class="btn leading-0 w-10 join-item" @click="deleteNode(i, bi)">
                                                <i class="bi bi-x-lg"></i>
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        </template>
                    </template>
                    <div v-else class="w-full h-54 pt-4">
                        <div
                            class="bg-base-200 w-full h-full rounded flex flex-col justify-center items-center gap-2 text-base-content/60"
                            tabindex="0"
                            @paste="handleEmptyStatePaste"
                        >
                            <i class="bi bi-upload text-9xl text-base-100"></i>
                            <p class="text-sm text-base-content/30">{{ $t('common.pasteStructureHint') }}</p>
                        </div>
                    </div>

                    <div v-if="templateCount > 0" class="flex justify-center mt-2">
                        <div class="grow flex justify-center">
                            <div class="join">
                                <button
                                    class="btn btn-sm btn-outline border-base-content/30 join-item rounded-l-full"
                                    @click="addTextNode()"
                                >
                                    {{ $t('common.text') }}
                                </button>
                                <button
                                    class="btn btn-sm btn-outline border-base-content/30 join-item"
                                    @click="openBlockModal(-1)"
                                >
                                    {{ $t('common.data') }}
                                </button>
                                <button
                                    class="btn btn-sm btn-outline border-base-content/30 join-item rounded-r-full"
                                    @click="addBlocksNode()"
                                >
                                    {{ $t('common.blocks') }}
                                </button>
                            </div>
                        </div>

                        <template v-if="isSupported">
                            <button v-if="copied" class="btn btn-sm btn-disabled">
                                <i class="bi bi-clipboard-check"></i>
                            </button>
                            <button
                                v-else
                                class="btn btn-sm"
                                :title="$t('common.copyStructureToClipboard')"
                                @click="copyStructure"
                            >
                                <i class="bi bi-copy"></i>
                            </button>
                        </template>
                    </div>
                </div>

                <div ref="editorEndRef" class="h-6 min-h-6"></div>
            </div>

            <div
                v-if="store.preview"
                class="grow max-w-200 hidden 2xl:flex flex-col mb-6 mt-4 bg-base-300 p-4 rounded overflow-hidden"
            >
                <MarkdownPreview v-if="content.nodes" :nodes="content.nodes" />
            </div>

            <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="deleteContent">
                <p>{{ $t('article.deleteConfirm', { type: store.routeType }) }}</p>
            </GenericModal>
            <MediaBrowser ref="mediaModal" :update="addMedia" :media-types="mediaTypeFilter" />
            <BlockModal
                ref="blockModal"
                @add-block="addDataNode"
                @template-count="(count) => (templateCount = count)"
            />
        </div>
    </div>
</template>
