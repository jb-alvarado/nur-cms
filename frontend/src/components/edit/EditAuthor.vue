<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import { useI18n } from 'vue-i18n'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { mediaPath } from '@/utils/helper'
import { slugify } from '@/utils/slugify.js'

import GenericModal from '@/components/generic/GenericModal.vue'
import MediaBrowser from '@/components/media/MediaBrowser.vue'
import TextEditor from '@/components/TextEditor.vue'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()
const route = useRoute()
const router = useRouter()

const deleteModal = ref()
const mediaModal = ref()
const authorId = Number(route.params.id ?? 0)
const author = ref({
    id: 0,
    first_name: '',
    last_name: '',
    slug: '',
    bio: undefined,
    media_id: undefined,
} as ContentAuthor)
const authorOriginal = ref(cloneDeep(author))
const imageFile = ref()
const media = ref<Media>()
const needsSave = computed(() => !isEqual(author.value, authorOriginal.value))

if (authorId > 0) {
    getAuthor()
}

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

const openMediaBrowser = () => {
    mediaModal.value.showModal()
}

async function getAuthor() {
    await fetch(`/api/content/authors?id=${authorId}&fields=id,first_name,bio,last_name,slug,media_id`, {
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
            author.value = response.results[0]
            authorOriginal.value = cloneDeep(author.value)

            if (author.value.media_id) {
                selectMedia()
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

async function selectMedia() {
    await fetch(`/api/media?id=${author.value.media_id}`, {
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
    author.value.slug = slugify(`${author.value.first_name} ${author.value.last_name}`)
}

async function savePhoto() {
    if (imageFile.value) {
        const formData = new FormData()
        formData.append(imageFile.value.name, imageFile.value)

        await fetch('/api/v2/file/upload/?type=thumbnail', {
            method: 'PUT',
            headers: auth.authHeader,
            body: formData,
        })
            .then(async (resp) => {
                if (resp.status >= 400) {
                    const msg = await errMsg(resp)
                    throw new Error(msg)
                }
                return Number(resp.text())
            })
            .then((response: number) => {
                author.value.media_id = response
            })
            .catch((e) => {
                store.msgAlert('error', e.data)

                return
            })
    }
}

function removeMedia() {
    author.value.media_id = null
    media.value = undefined
}

function contentDelete() {
    if (authorId > 0) {
        fetch(`/api/content/authors/${authorId}`, {
            method: 'DELETE',
            headers: auth.authHeader,
        })
            .then(async (resp) => {
                if (resp.status >= 400) {
                    const msg = await errMsg(resp)
                    throw new Error(msg)
                } else {
                    store.msgAlert('success', `Deleted: ${author.value.first_name ?? author.value.id}`)

                    router.push(`/author`)
                }
            })
            .catch((e) => {
                store.msgAlert('error', e)
            })
    }
}

async function save() {
    await savePhoto()

    const payload = Object.fromEntries(
        Object.entries(author.value).filter(([key, value]) => {
            return !isEqual(value, authorOriginal.value[key as keyof ContentAuthor])
        }),
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', t('common.noChanges'))
        return
    }

    fetch(`/api/content/authors${authorId > 0 ? `/${authorId}` : ''}`, {
        method: authorId > 0 ? 'PUT' : 'POST',
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

            if (authorId === 0) {
                await store.selectAuthors()
                router.push('/author')
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}

function addMedia(m: Media) {
    author.value.media_id = m.id
    media.value = m

    mediaModal.value.close()
}
</script>

<template>
    <div class="flex flex-col h-full pb-6">
        <div class="flex">
            <h1 class="grow text-2xl h-8">{{ author?.first_name ?? '' }} {{ author?.last_name ?? '' }}</h1>
            <button class="btn btn-sm text-base" @click="router.back()">
                <i class="bi bi-chevron-left" />
            </button>
        </div>

        <!-- Form + Editor Container -->
        <div
            v-if="author"
            class="flex flex-col flex-1 max-w-5xl min-h-96 bg-base-300 p-4 pt-1 mt-4 rounded overflow-hidden"
        >
            <!-- Form inputs -->
            <div class="flex items-center flex-wrap gap-2 flex-none">
                <div class="grow flex flex-col md:flex-row gap-2">
                    <fieldset class="fieldset max-w-80 md:max-w-56">
                        <legend class="fieldset-legend">{{ $t('user.firstName') }}</legend>
                        <input
                            v-model="author.first_name"
                            type="text"
                            class="input"
                            :placeholder="$t('user.firstName')"
                            @input="updateSlug()"
                        />
                    </fieldset>

                    <fieldset class="fieldset max-w-80 md:max-w-56">
                        <legend class="fieldset-legend">{{ $t('user.lastName') }}</legend>
                        <input
                            v-model="author.last_name"
                            type="text"
                            class="input"
                            :placeholder="$t('user.lastName')"
                            @input="updateSlug()"
                        />
                    </fieldset>

                    <fieldset class="fieldset max-w-80">
                        <legend class="fieldset-legend">{{ $t('article.slug') }}</legend>
                        <input v-model="author.slug" type="text" class="input" :placeholder="$t('article.slug')" />
                    </fieldset>
                </div>

                <div class="join md:mt-7">
                    <button class="btn text-warning join-item" @click="openDeleteModal()">
                        {{ $t('common.delete') }}
                    </button>
                    <button class="btn join-item" :class="{ 'btn-primary': needsSave }" @click="save()">
                        {{ $t('user.save') }}
                    </button>
                </div>
            </div>

            <div class="w-64 flex gap-1">
                <div
                    class="bg-checker w-53 aspect-video flex justify-center items-center border border-base-content/20"
                >
                    <img
                        v-if="media"
                        :src="mediaPath(media)"
                        :alt="media?.alt ?? $t('button.media')"
                        class="border border-base-content/30 max-h-26"
                    />
                </div>
                <div class="join join-vertical">
                    <button class="btn" @click="openMediaBrowser()">{{ $t('button.media') }}</button>
                    <button class="btn p-2 join-item" @click="removeMedia()">
                        <i class="bi bi-trash text-xl"></i>
                    </button>
                </div>
            </div>

            <TextEditor v-model="author.bio" />
        </div>

        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="contentDelete">
            <p>{{ $t('author.deleteConfirm') }}</p>
        </GenericModal>

        <MediaBrowser ref="mediaModal" :update="addMedia" />
    </div>
</template>
