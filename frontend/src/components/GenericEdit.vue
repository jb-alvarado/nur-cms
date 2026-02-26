<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import dayjs from 'dayjs'

import { useRoute, useRouter, RouterLink } from 'vue-router'
import { cloneDeep, isEqual } from 'lodash-es'
import Multiselect from 'vue-multiselect'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { closeDropdown, mediaPath } from '@/utils/helper'
import { slugify } from '@/utils/slugify.js'

import GenericBlock from './GenericBlock.vue'
import GenericModal from './GenericModal.vue'
import BlockModal from './BlockModal.vue'
import MarkdownRender from './MarkdownRender.vue'
import MediaBrowser from './MediaBrowser.vue'
import TextEditor from './TextEditor.vue'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const auth = useAuth()
const store = useIndex()

const rootPath = route.path.replace(/\/[0-9/]+$/g, '')

const contentId = Number(route.params.id ?? 0)
const groupID = Number(route.params.group_id ?? 0)
store.routeType = (Array.isArray(route.params.type) ? route.params.type[0] : route.params.type) ?? String(route.name)
const matchedType = store.types.find((type) => type.slug === store.routeType)?.id
store.typeID = matchedType ?? 1

const deleteModal = ref()
const mediaModal = ref()
const blockModal = ref()
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
                addDataNode({name: null,  media: null, data: json })
            }

            dropValueRaw.value = ''
        } catch {
            store.msgAlert('error', 'No valid json data!')
        }
    },
})
const content = ref({
    id: 0,
    group_id: groupID,
    type: '',
    title: '',
    slug: '',
    nodes: [],
    status: 'draft',
    locale_id: 0,
    group_members: [],
    check: false,
    meta: {},
} as Content)
const contentOriginal = ref(cloneDeep(content))

contentOriginal.value.group_id = 0
const media = ref<Media | null>(null)
const categories = ref<Category[]>([])
const tags = ref<Tag[]>([])
const locales = ref<Locale[]>([])
const needsSave = computed(() => !isEqual(content.value, contentOriginal.value))
const status = ['draft', 'published', 'archived']
const currentNodeIndex = ref(-1)
const templateCount = ref(0)

const authorsFormatted = computed(() =>
    store.authors.map((a) => ({
        ...a,
        displayName: `${a.first_name} ${a.last_name}`.trim(),
    })),
)

const selectedAuthorsFormatted = computed({
    get: () =>
        content.value.authors?.map((a) => ({
            ...a,
            displayName: `${a.first_name} ${a.last_name}`.trim(),
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
    fetch(
        `/api/content/entries?type_id=${store.typeID}&group_id=${groupID}&fields=locale_id,group_members&output_type=markdown`,
        {
            headers: auth.authHeader,
        },
    )
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
        .then((response: RespondObj) => {
            const groupMemberLocaleIds = new Set(
                response.results.flatMap(
                    (result: RespondObj) =>
                        result.group_members?.map((member: GroupMember) => member.locale_id) ?? [result.locale_id],
                ),
            )
            locales.value = store.locales.filter((locale) => !groupMemberLocaleIds.has(locale.id))
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

selectCategories()
selectTags()

function selectContent() {
    fetch(`/api/content/entries?type_id=${store.typeID}&id=${contentId}&output_type=markdown`, {
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
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
                }

                contentOriginal.value = cloneDeep(content.value)

                locales.value = store.locales.filter((locale) => {
                    const isCurrentLocale = locale.id === content.value.locale_id
                    const hasGroupMember = content.value.group_members?.some((member) => member.locale_id === locale.id)
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
    await fetch(`/api/media?id=${content.value.media_id}`, {
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
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

function updateSlug() {
    if (content.value.title) {
        content.value.slug = slugify(content.value.title)
    }
}

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

const openMediaBrowser = () => {
    mediaModal.value.showModal()
}

const openBlockModal = (index: number) => {
    currentNodeIndex.value = index
    blockModal.value.showModal()
}

function addTextNode() {
    content.value.nodes?.push({
        order_index: (content.value.nodes?.length ?? 0) + 1,
        text: '',
    })
}

function addBlocksNode() {
    content.value.nodes?.push({
        blocks: [],
    })
}

function sortBlocks(index: number) {
    const node = content.value.nodes?.[index] as { blocks: Array<ContentNodeSerializer> }
    if (node?.blocks) {
        const prevOrder = node.blocks.slice()
        node.blocks.sort((a, b) => (a.order_index ?? 0) - (b.order_index ?? 0))

        const orderChanged = node.blocks.some((block, i) => block !== prevOrder[i])
        if (orderChanged) {
            node.blocks.forEach((block, index) => {
                block.order_index = index + 1
            })
        }
    }
}

function addDataNode(item: { name: null | string, media: null | Media; data: Record<string, any> }) {
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
            media: item.media,
            order_index: (node.blocks?.length ?? 0) + 1,
        } as any)
    } else {
        content.value.nodes.push({
            media_id: item.media?.id ?? null,
            name: item.name,
            data: item.data,
            media: item.media,
            order_index: (content.value.nodes?.length ?? 0) + 1,
        } as any)
    }

    currentNodeIndex.value = -1
}

function deleteNode(index: number, blockIndex: number | null = null) {
    if (content.value.nodes) {
        if (blockIndex) {
            const node = content.value.nodes[index] as { blocks: Array<ContentNodeSerializer> }
            node.blocks.splice(blockIndex, 1)
        } else {
            content.value.nodes.splice(index, 1)
        }
    }
}

function memberLink(id: number): string {
    const member = content.value.group_members?.find((member) => member.locale_id === id)

    return `${rootPath}/${member?.id ?? content.value.id}`
}

async function save() {
    // Build payload with only changed fields
    const payload: Record<string, any> = Object.fromEntries(
        Object.entries(content.value as Record<string, any>).filter(([key, value]) => {
            return !isEqual(value, (contentOriginal.value as Record<string, any>)[key])
        }),
    )

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

    // Remove media from blocks
    if (payload.blocks) {
        payload.blocks = payload.blocks.map((item: any) => {
            delete item.media
            return item
        })
    }

    try {
        // Handle tag and author changes for existing entries
        if (contentId > 0) {
            await Promise.all([
                ...deletedTags.map((tag) => deleteEntryTag(contentId, tag.id!)),
                ...newTags.map((tag) => insertEntryTag(contentId, tag.id!)),
                ...deletedAuthors.map((author) => deleteEntryAuthor(author.id!)),
                ...newAuthors.map((author) => insertEntryAuthor(contentId, author.id!)),
            ])
        } else {
            payload.type_id = store.typeID
        }

        // Save entry if there are payload changes
        if (Object.keys(payload).length > 0) {
            const resp = await fetch(`/api/content/entries${contentId > 0 ? `/${contentId}` : ''}`, {
                method: contentId > 0 ? 'PUT' : 'POST',
                headers: {
                    ...auth.authHeader,
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify(payload),
            })

            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            store.msgAlert('success', t('common.saveSuccess'))

            // Handle new entry creation
            if (contentId === 0) {
                const newId = await resp.json()

                await Promise.all([
                    ...newTags.map((tag) => insertEntryTag(newId, tag.id!)),
                    ...newAuthors.map((author) => insertEntryAuthor(newId, author.id!)),
                ])

                router.push(`${rootPath}/${newId}`)
                return
            }
        }

        selectContent()
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}

function deleteContent() {
    if (contentId > 0) {
        fetch(`/api/content/entries/${contentId}`, {
            method: 'DELETE',
            headers: auth.authHeader,
        })
            .then(async (resp) => {
                if (resp.status >= 400) {
                    const msg = await errMsg(resp)
                    throw new Error(msg)
                } else {
                    store.msgAlert(
                        'success',
                        t('common.deleteSuccess', { name: content.value.title ?? content.value.id }),
                    )

                    router.push(rootPath)
                }
            })
            .catch((e) => {
                store.msgAlert('error', e)
            })
    }
}

function addMedia(m: Media) {
    content.value.media_id = m.id
    media.value = m

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

    fetch('/api/content/tags', {
        method: 'POST',
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            store.msgAlert('success', t('tag.saveSuccess'))

            await selectTags()
            payload.id = await resp.json()

            content.value.tags?.push(payload)
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function deleteEntryTag(entry_id: number, tag_id: number) {
    await fetch(`/api/content/entries/${entry_id}/tag/${tag_id}`, {
        method: 'DELETE',
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            } else {
                store.msgAlert('success', t('tag.deleteSuccess', { id: tag_id }))
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function insertEntryTag(entry: number, tag: number) {
    const payload = {
        entry_id: entry,
        tag_id: tag,
    }

    await fetch('/api/content/entries/tag', {
        method: 'POST',
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            store.msgAlert('success', t('tag.entrySaveSuccess'))
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function deleteEntryAuthor(author_id: number) {
    await fetch(`/api/content/authors/${author_id}`, {
        method: 'DELETE',
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            } else {
                store.msgAlert('success', t('author.deleteSuccess', { id: author_id }))
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function insertEntryAuthor(entry: number, author: number) {
    const payload = {
        entry_id: entry,
        author_id: author,
    }

    await fetch('/api/content/entries/author', {
        method: 'POST',
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            store.msgAlert('success', t('author.entrySaveSuccess'))
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div class="flex gap-2 h-full">
        <div class="flex flex-col h-full pb-6">
            <div class="flex-none">
                <h1 class="text-2xl h-8">{{ content?.title ?? '' }}</h1>
            </div>

            <!-- Form + Editor Container -->
            <div
                v-if="content"
                class="flex flex-col flex-1 max-w-5xl bg-base-300 px-4 pt-1 mt-4 rounded"
                :class="templateCount > 0 ? 'pb-2' : 'pb-4'"
            >
                <!-- Form inputs -->
                <div class="flex flex-wrap-reverse gap-4">
                    <div class="grow flex flex-col md:flex-row gap-2">
                        <fieldset class="fieldset w-64">
                            <legend class="fieldset-legend">{{ $t('table.title') }}</legend>
                            <input
                                v-model="content.title"
                                type="text"
                                class="input"
                                name="title"
                                :placeholder="$t('table.title')"
                                @input="updateSlug()"
                            />
                        </fieldset>

                        <fieldset class="fieldset w-64">
                            <legend class="fieldset-legend">{{ $t('article.slug') }}</legend>
                            <input v-model="content.slug" type="text" class="input" :placeholder="$t('article.slug')" />
                        </fieldset>
                    </div>

                    <div class="mt-3 md:mt-8 flex gap-2 flex-none">
                        <div class="join">
                            <details v-if="content.id === 0" class="dropdown">
                                <summary class="btn join-item" @blur="closeDropdown">
                                    {{
                                        store.locales.find((l) => l.id === content.locale_id)?.name ||
                                        $t('common.language')
                                    }}
                                </summary>
                                <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-2 shadow-sm">
                                    <li v-for="l in locales" :key="l.id">
                                        <a @click="content.locale_id = l.id">{{ l.name }}</a>
                                    </li>
                                </ul>
                            </details>

                            <details v-if="(content.id ?? 0) > 0" class="dropdown">
                                <summary class="btn join-item" @blur="closeDropdown">
                                    {{ store.locales.find((l) => l.id === content.locale_id)?.name }}
                                </summary>
                                <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-2 shadow-sm">
                                    <li v-for="l in locales" :key="l.id">
                                        <RouterLink :to="memberLink(l.id!)">{{ l.name }}</RouterLink>
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

                            <details class="dropdown">
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
                                <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-24 p-2 shadow-sm">
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

                        <div class="join">
                            <button class="btn text-warning join-item" @click="openDeleteModal()">
                                {{ $t('common.delete') }}
                            </button>
                            <button class="btn join-item" :class="{ 'btn-primary': needsSave }" @click="save()">
                                {{ $t('user.save') }}
                            </button>
                        </div>
                    </div>
                </div>

                <div class="flex flex-col md:flex-row gap-2 mt-2">
                    <div class="w-64 flex gap-1">
                        <div
                            class="bg-checker w-53 aspect-video flex justify-center items-center border border-base-content/20"
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
                            <fieldset class="fieldset py-0 grow min-w-64">
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
                            <fieldset class="fieldset py-0 grow min-w-46">
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

                        <fieldset class="fieldset py-0">
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

                        <div
                            v-if="content.meta?.start_time || store.routeType === 'event'"
                            class="flex flex-wrap gap-2"
                        >
                            <fieldset class="flex-1 fieldset py-0 min-w-50">
                                <legend class="fieldset-legend pt-0">{{ $t('common.start') }}</legend>
                                <input v-model="content.meta!.start_time" type="datetime-local" class="input w-full" />
                            </fieldset>
                            <fieldset class="flex-1 fieldset py-0 min-w-50">
                                <legend class="fieldset-legend pt-0">{{ $t('common.end') }}</legend>
                                <input v-model="content.meta!.end_time" type="datetime-local" class="input w-full" />
                            </fieldset>
                        </div>
                    </div>
                </div>

                <!-- Nodes -->

                <template v-for="(node, i) in content.nodes" :key="i">
                    <TextEditor
                        v-if="!('blocks' in node) && !('data' in node)"
                        v-model="node.text"
                        :remove-node="templateCount > 0 ? () => deleteNode(i) : null"
                    />
                    <div v-else-if="'data' in node" class="bg-base-200 rounded mt-2 p-2 flex gap-1">
                        <div class="w-10">
                            <img
                                v-if="node.media_id"
                                :src="mediaPath(node.media!)"
                                :atl="node.media?.alt"
                                class="object-cover w-10 h-10"
                            />
                            <div v-else class="bg-base-content/30 w-full h-10"></div>
                        </div>
                        <GenericBlock v-model:block="node.data" class="grow" />
                        <button class="btn leading-0 w-10" @click="deleteNode(i)">
                            <i class="bi bi-x-lg"></i>
                        </button>
                    </div>
                    <div v-else-if="'blocks' in node">
                        <div class="flex mt-4 items-center">
                            <h3 class="text-xl">{{ $t('common.blocks') }}</h3>
                            <div class="grow flex justify-end">
                                <div class="join">
                                    <button
                                        class="btn btn-sm"
                                        :title="$t('common.newBlock')"
                                        @click="openBlockModal(i)"
                                    >
                                        <i class="bi bi-plus-lg scale-130"></i>
                                    </button>
                                    <button class="btn btn-sm" :title="$t('common.removeBlock')" @click="deleteNode(i)">
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
                                class="bg-base-200 rounded p-2 flex gap-1"
                            >
                                <div class="w-10">
                                    <img
                                        v-if="block.media"
                                        :src="mediaPath(block.media!)"
                                        :atl="block.media?.alt"
                                        class="object-cover w-10 h-10"
                                    />
                                    <div v-else class="bg-base-content/30 w-full h-10"></div>
                                </div>
                                <GenericBlock v-model:block="block.data" class="grow" />
                                <input
                                    v-model="block.order_index"
                                    type="number"
                                    class="input w-15"
                                    @change="sortBlocks(i)"
                                />
                                <button class="btn leading-0 w-10" @click="deleteNode(i, bi)">
                                    <i class="bi bi-x-lg"></i>
                                </button>
                            </div>
                        </div>
                    </div>
                </template>

                <div v-if="templateCount > 0" class="flex justify-center mt-2">
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
            </div>
        </div>

        <div
            v-if="store.preview"
            class="grow max-w-200 hidden 2xl:flex flex-col mb-6 mt-12 bg-base-300 p-4 rounded overflow-hidden"
        >
            <MarkdownRender v-if="content.nodes" :nodes="content.nodes" />
        </div>

        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="deleteContent">
            <p>{{ $t('article.deleteConfirm', { type: store.routeType }) }}</p>
        </GenericModal>
        <MediaBrowser ref="mediaModal" :update="addMedia" />
        <BlockModal ref="blockModal" @add-block="addDataNode" @template-count="(count) => (templateCount = count)" />
    </div>
</template>
