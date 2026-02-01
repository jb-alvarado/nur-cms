<script setup lang="ts">
import { ref, computed, useTemplateRef, nextTick } from 'vue'
import { useEventListener } from '@vueuse/core'
import { useI18n } from 'vue-i18n'
import dayjs from 'dayjs'
import { useSortable } from '@vueuse/integrations/useSortable'

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
const blockEL = useTemplateRef('blockEL')
const dropField = useTemplateRef('dropField')
const content = ref({
    id: 0,
    group_id: groupID,
    type: '',
    title: '',
    slug: '',
    description: '',
    text: '',
    status: 'draft',
    locale_id: 0,
    group_members: [],
    check: false,
    meta: {},
} as Content)
const contentOriginal = ref(cloneDeep(content))

useSortable(blockEL, content.value.blocks ?? [], {
    onUpdate: (e: any) => {
        const blocks = content.value.blocks ?? []
        const [movedBlock] = blocks.splice(e.oldIndex, 1)
        if (movedBlock) {
            blocks.splice(e.newIndex, 0, movedBlock)
        }

        nextTick(() => {
            content.value.blocks?.forEach((block, index) => {
                block.order_index = index + 1
            })
        })
    },
})

useEventListener(dropField, 'paste', (e) => {
    const text = e.clipboardData?.getData('text')

    if (text) {
        try {
            const json = JSON.parse(text)

            if (Array.isArray(json)) {
                for (const obj of json) {
                    addBlock({ media: null, block: obj })
                }
            } else {
                addBlock({ media: null, block: json })
            }
        } catch {
            store.msgAlert('error', 'No valid json data!')
        }
    }

    if (dropField.value) {
        dropField.value.value = ''
    }
})

contentOriginal.value.group_id = 0
const media = ref<Media | null>(null)
const categories = ref<Category[]>([])
const tags = ref<Tag[]>([])
const locales = ref<Locale[]>([])
const needsSave = computed(() => !isEqual(content.value, contentOriginal.value))
const status = ['draft', 'published', 'archived']

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
    setTimeout(() => {
        locales.value = store.locales
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

const openBlockModal = () => {
    blockModal.value.showModal()
}

function addBlock(item: { media: null | Media; block: Record<string, any> }) {
    if (!content.value.blocks) {
        content.value.blocks = []
    }

    content.value.blocks.push({
        media_id: item.media?.id ?? null,
        data: item.block,
        media: item.media,
    } as any)

    content.value.blocks?.forEach((block, index) => {
        block.order_index = index + 1
    })
}

function updateDescription() {
    if (!content.value.text) return

    const textWithoutFrontmatter = content.value.text
        .split('\n')
        .filter((line: string) => {
            const trimmed = line.trim()
            return (
                !trimmed.startsWith('#') &&
                !trimmed.startsWith('![') &&
                !trimmed.startsWith('[') &&
                !trimmed.startsWith('<') &&
                !trimmed.startsWith('>')
            )
        })
        .join('\n')
        .trim()

    const excerpt = textWithoutFrontmatter.slice(0, 255)

    if (!content.value.description && excerpt.length > 160) {
        content.value.description = excerpt
    }
}

function memberLink(id: number): string {
    const member = content.value.group_members?.find((member) => member.locale_id === id)

    return `${rootPath}/${member?.id ?? content.value.id}`
}

async function save() {
    // Build payload with only changed fields
    const payload = Object.fromEntries(
        Object.entries(content.value).filter(([key, value]) => {
            return !isEqual(value, contentOriginal.value[key as keyof Content])
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
                ...deletedAuthors.map((author) => deleteEntryAuthor(contentId, author.id!)),
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

async function deleteEntryAuthor(entry_id: number, author_id: number) {
    await fetch(`/api/content/entries/${entry_id}/author/${author_id}`, {
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

function deleteBlock(index: number) {
    if (content.value.blocks) {
        content.value.blocks.splice(index, 1)
    }
}
</script>

<template>
    <div class="flex gap-2 h-full">
        <div class="flex flex-col h-full pb-6">
            <div class="flex-none">
                <h1 class="text-2xl h-8">{{ content?.title ?? '' }}</h1>
            </div>

            <!-- Form + Editor Container -->
            <div v-if="content" class="flex flex-col flex-1 max-w-5xl bg-base-300 p-4 pt-1 mt-4 rounded">
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

                            <!-- <button class="btn btn-disabled bg-base-300 p-1"></button> -->

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

                <div class="w-full">
                    <fieldset class="fieldset">
                        <legend class="fieldset-legend">{{ $t('article.description') }}</legend>
                        <textarea
                            v-model="content.description"
                            class="textarea h-20 w-full"
                            :placeholder="$t('article.description')"
                        ></textarea>
                    </fieldset>
                </div>

                <!-- Toolbar -->
                <TextEditor v-model="content.text" :update="updateDescription" />

                <div class="flex mt-4 items-center">
                    <h3 class="text-xl">Blocks</h3>
                    <div class="grow flex justify-end">
                        <button class="btn btn-sm" title="New Block" @click="openBlockModal()">
                            <i class="bi bi-plus-lg text-xl"></i>
                        </button>
                    </div>
                </div>

                <div ref="blockEL">
                    <div v-if="!content.blocks || content.blocks?.length === 0" class="bg-base-200 w-full min-h-6 mt-2">
                        <input ref="dropField" class="w-full h-full focus:outline-0 text-base-content/0 cursor-default" />
                    </div>
                    <div
                        v-for="(block, i) in content.blocks"
                        :key="block.id ?? i"
                        class="bg-base-200 rounded mt-2 p-2 flex gap-1 cursor-grab active:cursor-grabbing"
                    >
                        <div class="w-10">
                            <img
                                v-if="block.media_id"
                                :src="mediaPath(block.media!)"
                                :atl="block.media?.alt"
                                class="object-cover w-10 h-10"
                            />
                            <div v-else class="bg-base-content/30 w-full h-10"></div>
                        </div>
                        <GenericBlock v-model:block="block.data" class="grow" />
                        <button class="btn leading-0 w-10" @click="deleteBlock(i)">
                            <i class="bi bi-x-lg"></i>
                        </button>
                    </div>
                </div>
            </div>
        </div>

        <div
            v-if="store.preview"
            class="grow max-w-200 hidden 2xl:flex flex-col mb-6 mt-12 bg-base-300 p-4 rounded overflow-hidden"
        >
            <MarkdownRender v-if="content.text" :text="content.text" />
        </div>

        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="deleteContent">
            <p>{{ $t('article.deleteConfirm', { type: store.routeType }) }}</p>
        </GenericModal>
        <MediaBrowser ref="mediaModal" :update="addMedia" />
        <BlockModal ref="blockModal" @add-block="addBlock" />
    </div>
</template>
