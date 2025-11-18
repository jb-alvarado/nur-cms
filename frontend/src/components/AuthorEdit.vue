<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { cloneDeep, isEqual } from 'lodash-es'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { slugify } from '@/utils/slugify.js'

import ImageSelection from './ImageSelection.vue'
import TextEditor from '@/components/TextEditor.vue'

const auth = useAuth()
const store = useIndex()
const route = useRoute()
const router = useRouter()
const authorId = Number(route.params.id ?? 0)
const author = ref({
    id: 0,
    first_name: '',
    last_name: '',
    slug: '',
    bio: undefined,
    photo: undefined,
} as ContentAuthor)
const authorOriginal = ref(cloneDeep(author))
const imageFile = ref()
const needsSave = computed(() => !isEqual(author.value, authorOriginal.value))

if (authorId > 0) {
    getAuthor()
}

async function getAuthor() {
    await fetch(`/api/content/authors?id=${authorId}`, {
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
            headers: { ...auth.authHeader },
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

// TODO: When new author is added, load authors new to store

async function save() {
    await savePhoto()

    const payload = Object.fromEntries(
        Object.entries(author.value).filter(([key, value]) => {
            return !isEqual(value, authorOriginal.value[key as keyof ContentAuthor])
        })
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', 'No changes to save')
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
            store.msgAlert('success', 'Content saved successfully')

            if (authorId === 0) {
                router.push(`/author/${await resp.text()}`)
            }
        })
        .catch((e) => {
            store.msgAlert('error', e)
        })
}
</script>

<template>
    <div class="flex flex-col h-full pb-6">
        <div class="flex-none">
            <h1 class="text-2xl h-8">{{ author?.first_name ?? '' }} {{ author?.last_name ?? '' }}</h1>
        </div>

        <!-- Form + Editor Container -->
        <div
            v-if="author"
            class="flex flex-col flex-1 max-w-5xl min-h-96 bg-base-300 p-4 pt-1 mt-4 rounded overflow-hidden"
        >
            <!-- Form inputs -->
            <div class="flex items-center flex-wrap gap-4 flex-none">
                <div class="grow flex flex-col md:flex-row gap-2">
                    <fieldset class="fieldset w-full max-w-80">
                        <legend class="fieldset-legend">First Name</legend>
                        <input
                            v-model="author.first_name"
                            type="text"
                            class="input"
                            placeholder="First Name"
                            @input="updateSlug()"
                        />
                    </fieldset>

                    <fieldset class="fieldset w-full max-w-80">
                        <legend class="fieldset-legend">Last Name</legend>
                        <input
                            v-model="author.last_name"
                            type="text"
                            class="input"
                            placeholder="Last Name"
                            @input="updateSlug()"
                        />
                    </fieldset>
                </div>

                <button class="btn mt-7" :class="{ 'btn-primary': needsSave }" @click="save()">Save</button>
            </div>

            <div class="grow flex flex-col md:flex-row gap-2">
                <fieldset class="fieldset w-64">
                    <legend class="fieldset-legend">Slug</legend>
                    <input v-model="author.slug" type="text" class="input" placeholder="Slug" />
                </fieldset>
                <div class="w-full max-w-96">
                    <ImageSelection
                        v-model:path="author.media_id"
                        v-model:image="imageFile"
                        title="Photo"
                        placeholder="Photo"
                    />
                </div>
            </div>

            <TextEditor v-model="author.bio" />
        </div>
    </div>
</template>
