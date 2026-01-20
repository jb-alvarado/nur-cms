<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { cloneDeep, isEqual } from 'lodash-es'
import { useAuth } from '@/stores/auth'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import { closeDropdown, mediaPath } from '@/utils/helper'
import { errMsg } from '@/utils/error'
import { slugify } from '@/utils/slugify.js'

import GenericModal from '@/components/GenericModal.vue'
import MediaBrowser from '@/components/MediaBrowser.vue'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()
const route = useRoute()
const router = useRouter()
const categoryId = Number(route.params.id ?? 0)
const groupID = Number(route.params.group_id ?? 0)
const deleteModal = ref()
const mediaModal = ref()
const category = ref({
    id: 0,
    group_id: groupID,
    locale_id: 0,
    name: '',
    slug: '',
    status: 'draft',
    media_id: 0,
} as ContentCategory)
const categoryOriginal = ref(cloneDeep(category))
const media = ref<Media>()
categoryOriginal.value.group_id = 0
const locales = ref<Locale[]>([])
const needsSave = computed(() => {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { group_id: _, ...categoryWithoutGroupId } = category.value
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { group_id: __, ...originalWithoutGroupId } = categoryOriginal.value
    return !isEqual(categoryWithoutGroupId, originalWithoutGroupId)
})
const status = ['draft', 'published']

if (categoryId > 0) {
    selectCategory()
} else if (groupID > 0) {
    fetch(`/api/content/categories?group_id=${groupID}&fields=locale_id,group_members`, {
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
            store.msgAlert('error', e)
        })
} else {
    setTimeout(() => {
        locales.value = store.locales
    }, 1000)
}

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

const openMediaBrowser = () => {
    mediaModal.value.showModal()
}

async function selectCategory() {
    await fetch(`/api/content/categories?id=${categoryId}`, {
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
            category.value = response.results[0]
            categoryOriginal.value = cloneDeep(category.value)

            locales.value = store.locales.filter((locale) => {
                const isCurrentLocale = locale.id === category.value.locale_id
                const hasGroupMember = category.value.group_members?.some(
                    (member: GroupMember) => member.locale_id === locale.id
                )
                return isCurrentLocale || hasGroupMember
            })

            if (category.value.media_id) {
                selectMedia()
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectMedia() {
    await fetch(`/api/media?id=${category.value.media_id}`, {
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

function updateSlug() {
    category.value.slug = slugify(category.value.name ?? '')
}

function memberLink(id: number): string {
    const member = category.value.group_members?.find((member: GroupMember) => member.locale_id === id)

    return `/content/category/${member?.id ?? category.value.id}`
}

function contentDelete() {
    if (categoryId > 0) {
        fetch(`/api/content/categories/${categoryId}`, {
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
                        t('common.deleteSuccess', { name: category.value.name ?? category.value.id })
                    )

                    router.push(`/category`)
                }
            })
            .catch((e) => {
                store.msgAlert('error', e)
            })
    }
}

async function save() {
    const payload = Object.fromEntries(
        Object.entries(category.value).filter(([key, value]) => {
            return !isEqual(value, categoryOriginal.value[key as keyof ContentCategory])
        })
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', t('common.noChanges'))
        return
    }

    if (categoryId === 0 && !payload.locale_id) {
        store.msgAlert('warning', t('common.selectLanguage'))
        return
    }

    fetch(`/api/content/categories${categoryId > 0 ? `/${categoryId}` : ''}`, {
        method: categoryId > 0 ? 'PUT' : 'POST',
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
            store.msgAlert('success', t('common.saveSuccess'))

            if (categoryId === 0) {
                router.push(`/category/${await resp.text()}`)
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

function addMedia(m: Media) {
    category.value.media_id = m.id
    media.value = m

    mediaModal.value.close()
}
</script>

<template>
    <div class="flex flex-col md:h-96 pb-6">
        <div class="flex-none">
            <h1 class="text-2xl h-8">{{ category?.name ?? '' }}</h1>
        </div>

        <!-- Form + Editor Container -->
        <div
            v-if="category"
            class="flex flex-col flex-1 max-w-5xl min-h-96 bg-base-300 p-4 pt-1 mt-4 rounded overflow-hidden"
        >
            <!-- Form inputs -->
            <div class="flex items-center flex-wrap gap-4 flex-none">
                <div class="grow flex flex-col md:flex-row gap-2">
                    <fieldset class="fieldset w-full max-w-80">
                        <legend class="fieldset-legend">{{ $t('article.name') }}</legend>
                        <input
                            v-model="category.name"
                            type="text"
                            class="input"
                            :placeholder="$t('article.name')"
                            @input="updateSlug()"
                        />
                    </fieldset>

                    <fieldset class="fieldset w-full max-w-64">
                        <legend class="fieldset-legend">{{ $t('article.slug') }}</legend>
                        <input v-model="category.slug" type="text" class="input" :placeholder="$t('article.slug')" />
                    </fieldset>
                </div>

                <div class="md:mt-7 flex flex-col md:flex-row gap-2">
                    <div class="join">
                        <details v-if="category.id === 0" class="dropdown">
                            <summary class="btn join-item" @blur="closeDropdown">
                                {{
                                    store.locales.find((l) => l.id === category.locale_id)?.name ||
                                    $t('common.language')
                                }}
                            </summary>
                            <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-2 shadow-sm">
                                <li v-for="l in locales" :key="l.id">
                                    <a @click="category.locale_id = l.id">{{ l.name }}</a>
                                </li>
                            </ul>
                        </details>

                        <details v-if="(category.id ?? 0) > 0" class="dropdown">
                            <summary class="btn join-item" @blur="closeDropdown">
                                {{ store.locales.find((l) => l.id === category.locale_id)?.name }}
                            </summary>
                            <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-2 shadow-sm">
                                <li v-for="l in locales" :key="l.id">
                                    <RouterLink :to="memberLink(l.id!)">{{ l.name }}</RouterLink>
                                </li>
                            </ul>
                        </details>

                        <RouterLink
                            :to="`/content/category/0/${category.group_id}`"
                            class="btn join-item px-2"
                            :title="$t('common.addLanguage')"
                        >
                            <i class="bi bi-plus-lg"></i>
                        </RouterLink>

                        <button class="btn btn-disabled bg-base-300 p-1"></button>
                        <details class="dropdown">
                            <summary
                                class="btn join-item"
                                :class="{
                                    'text-success': category.status === 'published',
                                }"
                                @blur="closeDropdown"
                            >
                                {{ category.status }}
                            </summary>
                            <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-24 p-2 shadow-sm">
                                <li
                                    v-for="s in status"
                                    :key="s"
                                    :class="{
                                        'text-base-content/50': category.status !== s,
                                    }"
                                >
                                    <a @click="category.status = s">{{ s }}</a>
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

            <div class="flex gap-2 mt-2">
                <img
                    v-if="media"
                    :src="mediaPath(media)"
                    :alt="media?.alt ?? $t('button.media')"
                    class="border border-base-content/30 max-h-26"
                />
                <button class="btn" @click="openMediaBrowser()">{{ $t('button.media') }}</button>
            </div>
        </div>
        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="contentDelete">
            <p>{{ $t('category.deleteConfirm') }}</p>
        </GenericModal>

        <MediaBrowser ref="mediaModal" :update="addMedia" />
    </div>
</template>
