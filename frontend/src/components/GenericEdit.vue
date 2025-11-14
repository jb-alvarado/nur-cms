<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute, useRouter, RouterLink } from 'vue-router'
import { cloneDeep, isEqual } from 'lodash-es'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { closeDropdown } from '@/utils/helper'
import { slugify } from '@/utils/slugify.js'

import GenericModal from '@/components/GenericModal.vue'
import TextEditor from '@/components/TextEditor.vue'

const route = useRoute()
const router = useRouter()
const contentId = Number(route.params.id ?? 0)
const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type
const groupID = Number(route.params.group_id ?? 0)

const auth = useAuth()
const store = useIndex()
const deleteModal = ref()
const content = ref({
    id: 0,
    group_id: groupID,
    type: '',
    title: '',
    slug: '',
    description: '',
    body: '',
    status: 'draft',
    locale_id: 0,
    group_members: [],
    check: false,
} as Content)
const contentOriginal = ref(cloneDeep(content))
contentOriginal.value.group_id = 0

const locales = ref<Locale[]>([])
const needsSave = computed(() => !isEqual(content.value, contentOriginal.value))
const status = ['draft', 'published', 'archived']

if (contentId > 0) {
    getContent()
} else if (groupID > 0) {
    fetch(`/api/content/entries/${typeParam}?group_id=${groupID}&fields=locale_id,group_members&output_type=markdown`, {
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
            const groupMemberLocaleIds = new Set(
                response.results.flatMap(
                    (result: RespondObj) =>
                        result.group_members?.map((member: GroupMember) => member.locale_id) ?? [result.locale_id]
                )
            )
            locales.value = store.locales.filter((locale) => !groupMemberLocaleIds.has(locale.id))
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
} else {
    setTimeout(() => {
        locales.value = store.locales
    }, 1000)
}

function getContent() {
    fetch(`/api/content/entries/${typeParam}?id=${contentId}&output_type=markdown`, {
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
                contentOriginal.value = cloneDeep(content.value)

                locales.value = store.locales.filter((locale) => {
                    const isCurrentLocale = locale.id === content.value.locale_id
                    const hasGroupMember = content.value.group_members?.some((member) => member.locale_id === locale.id)
                    return isCurrentLocale || hasGroupMember
                })
            }
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
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

function updateDescription() {
    if (!content.value.body) return

    const bodyWithoutFrontmatter = content.value.body
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

    const excerpt = bodyWithoutFrontmatter.slice(0, 255)

    if (!content.value.description && excerpt.length > 160) {
        content.value.description = excerpt
    }
}

function memberLink(id: number): string {
    const member = content.value.group_members?.find((member) => member.locale_id === id)

    return `/${typeParam}/${member?.id ?? content.value.id}`
}

function save() {
    const payload = Object.fromEntries(
        Object.entries(content.value).filter(([key, value]) => {
            return !isEqual(value, contentOriginal.value[key as keyof Content])
        })
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', 'No changes to save', 3)
        return
    }

    if (contentId === 0 && !payload.locale_id) {
        store.msgAlert('warning', 'Select a language', 3)
        return
    }

    fetch(`/api/content/entries${contentId > 0 ? `/${contentId}` : `/${typeParam}`}`, {
        method: contentId > 0 ? 'PUT' : 'POST',
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
            store.msgAlert('success', 'Content saved successfully', 3)

            if (contentId === 0) {
                router.push(`/${typeParam}/${await resp.text()}`)
            }
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
}

function contentDelete() {
    const auth = useAuth()
    const url = store.baseURL.replace(/\/[^/]+$/, '')

    if (contentId > 0) {
        fetch(`${url}/${contentId}`, {
            method: 'DELETE',
            headers: auth.authHeader,
        })
            .then(async (resp) => {
                if (resp.status >= 400) {
                    const msg = await errMsg(resp)
                    throw new Error(msg)
                } else {
                    store.msgAlert('success', `Deleted: ${content.value.title ?? content.value.id}`, 2)

                     router.push(`/${typeParam}`)
                }
            })
            .catch((e) => {
                store.msgAlert('error', e, 6)
            })
    }
}
</script>

<template>
    <div class="flex flex-col h-full pb-6">
        <div class="flex-none">
            <h1 class="text-2xl h-8">{{ content?.title ?? '' }}</h1>
        </div>

        <!-- Form + Editor Container -->
        <div
            v-if="content"
            class="flex flex-col flex-1 max-w-5xl min-h-96 bg-base-300 p-4 pt-1 mt-4 rounded overflow-hidden"
        >
            <!-- Form inputs -->
            <div class="flex items-center flex-wrap gap-4 flex-none">
                <div class="grow flex flex-col md:flex-row gap-2">
                    <fieldset class="fieldset w-64">
                        <legend class="fieldset-legend">Title</legend>
                        <input
                            v-model="content.title"
                            type="text"
                            class="input"
                            placeholder="Title"
                            @input="updateSlug()"
                        />
                    </fieldset>

                    <fieldset class="fieldset w-64">
                        <legend class="fieldset-legend">Slug</legend>
                        <input v-model="content.slug" type="text" class="input" placeholder="Slug" />
                    </fieldset>
                </div>

                <div class="md:mt-7 flex gap-2 flex-none">
                    <div class="join">
                        <details v-if="content.id === 0" class="dropdown">
                            <summary class="btn join-item" @blur="closeDropdown">
                                {{ store.locales.find((l) => l.id === content.locale_id)?.name || 'Language' }}
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
                            :to="`/${typeParam}/0/${content.group_id}`"
                            class="btn join-item px-2"
                            title="Add Language"
                        >
                            <i class="bi bi-plus-lg"></i>
                        </RouterLink>

                        <button class="btn btn-disabled bg-base-300 p-1"></button>

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
                        <button class="btn btn-warning join-item" @click="openDeleteModal()">Delete</button>
                        <button class="btn join-item" :class="{ 'btn-primary': needsSave }" @click="save()">
                            Save
                        </button>
                    </div>
                </div>
            </div>

            <div class="w-full">
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">Description</legend>
                    <textarea
                        v-model="content.description"
                        class="textarea h-20 w-full"
                        placeholder="Description"
                    ></textarea>
                </fieldset>
            </div>

            <!-- Toolbar -->
            <TextEditor v-model="content.body" :update="updateDescription" />
        </div>

        <GenericModal ref="deleteModal" title="Delete Selection" :ok-action="contentDelete">
            <p>Are you sure you want to delete this {{ typeParam }}?</p>
        </GenericModal>
    </div>
</template>
