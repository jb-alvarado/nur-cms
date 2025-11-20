<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import dayjs from 'dayjs'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { formatBytes, shortID } from '@/utils/helper'

import FileUpload from '@/components/FileUpload.vue'
import GenericModal from '@/components/GenericModal.vue'
import GenericPagination from '@/components/GenericPagination.vue'
import GenericProgress from '@/components/GenericProgress.vue'

const auth = useAuth()
const store = useIndex()
const deleteModal = ref()
const uploadModal = ref()
const uploader = ref()
const medias = ref<Media[]>([])
const selectCount = computed(() => medias.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const uploadKey = ref(shortID())

const apiURL = ref('/api/media')
const total = ref(0)
const limit = ref(20)
const offset = ref(0)
const offsetVar = computed({
    get() {
        return offset.value > 0 ? `&offset=${offset.value}` : ''
    },
    set(newValue) {
        offset.value = Number(newValue)
    },
})
const limits = [2, 10, 20, 50]
const search = ref('')
const searchVar = computed({
    get() {
        return search.value ? `&search=${search.value}` : ''
    },
    set(newValue) {
        search.value = newValue
    },
})

const ordering = ref('-created_at')
const previous = ref('')
const next = ref('')

watch(
    () => store.progress,
    (newVal) => {
        if (newVal === 100) {
            setTimeout(() => {
                selectMedia()
            }, 3000)
        }
    }
)

selectMedia()

const openDeleteModal = () => {
    deleteModal.value.showModal()
}

async function selectMedia(u: string | null = null) {
    const url = u
        ? u
        : apiURL.value + `?limit=${limit.value}${searchVar.value}${offsetVar.value}&ordering=${ordering.value}`

    await fetch(url, { headers: auth.authHeader })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            return resp.json()
        })
        .then(async (res) => {
            if (res.results?.length > 0) {
                total.value = res.count
                medias.value = res.results
                previous.value = res.previous
                next.value = res.next
            } else {
                medias.value = []
                previous.value = ''
                next.value = ''
            }
        })
        .catch((err) => {
            store.msgAlert('error', err)
        })
}

async function contentMedia() {
    for (const item of medias.value) {
        if (item.check) {
            await fetch(`/api/media/${item.id}`, {
                method: 'DELETE',
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    } else {
                        store.msgAlert('success', `Deleted: ${item.filename ?? item.id}`)
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e)
                })
        }
    }

    await selectMedia()
}

function onPageChange(/*payload: any*/) {
    selectMedia()
}

function mediaPath(media: Media): string {
    if (media.variants && media.variants.length > 0) {
        const variance320 = media.variants.find((v) => v.width === 320)
        if (variance320) {
            return `${media.path}/${variance320.filename}`
        }
    }
    return `${media.path}/${media.filename}`
}

function iconFrom(type: string | null | undefined) {
    const t = (type || '').toLowerCase()
    switch (true) {
        case t.includes('zip'):
            return 'bi-file-earmark-zip'
        case t.includes('pdf'):
            return 'bi-file-earmark-pdf'
        case t.includes('audio'):
            return 'bi-file-earmark-music'
        case t.includes('video'):
            return 'bi-file-earmark-play'
        case t.includes('text'):
            return 'bi-file-earmark-text'
        case t.includes('powerpoint'):
            return 'bi-file-earmark-ppt'
        case t.includes('word'):
            return 'bi-file-earmark-word'
        default:
            return 'bi-file-earmark'
    }
}

function extension(input: string | null | undefined) {
    const extensionMatch = input?.match(/\.([^.]+)$/)
    return extensionMatch?.[1]?.toUpperCase()
}

function mimeType(media: Media) {
    const typeMatch = media.type?.match(/\/([^.]+)$/)
    const type = typeMatch?.[1]?.toUpperCase()
    const ext = extension(media.filename)
    return type ? type : ext ? ext : 'File'
}

function variantsDim(variants: Variants[]) {
    const dims = new Set<string>()

    for (const v of variants) {
        const dim = `${v.width}x${v.height}`
        dims.add(dim)
    }

    return Array.from(dims).join(' | ')
}

function variantsExt(variants: Variants[]) {
    const exts = new Set<string>()

    for (const v of variants) {
        const ext = extension(v.filename)
        if (!ext) continue
        exts.add(ext)
    }

    return Array.from(exts).join(' | ')
}

function runUpload() {
    if (uploader.value) {
        uploader.value.upload()
    }
}

function resetUpload() {
    uploadKey.value = shortID()
}
</script>
<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">Media</h1>
            <button class="btn btn-sm btn-primary text-base" @click="uploadModal.showModal()">Upload</button>
        </div>

        <div class="h-10 mt-4 mb-6 flex items-center">
            <div class="grow join">
                <label class="input" :class="selectCount > 0 ? 'w-40' : 'w-74'">
                    <i class="bi bi-search opacity-45"></i>
                    <input v-model="search" type="search" placeholder="Search" @keyup="selectMedia()" />
                </label>
                <div v-if="selectCount > 0">
                    <button class="btn text-warning join-item" @click="openDeleteModal">Delete</button>
                    <span class="ms-2">{{ selectCount }} Selected</span>
                </div>
            </div>

            <GenericPagination
                v-model:limit="limit"
                v-model:offset="offset"
                :total="total"
                :page-sizes="limits"
                @change="onPageChange"
            />
        </div>

        <div class="flex gap-4 flex-wrap justify-start p-4 mt-4">
            <div
                v-for="media in medias"
                :key="media.id ?? media.filename!"
                class="card bg-base-100 w-64 shadow-sm rounded border border-base-content/20 hover:scale-[1.01] hover:shadow-md transition-transform cursor-pointer"
            >
                <figure class="relative checker h-43">
                    <input
                        v-model="media.check"
                        type="checkbox"
                        class="checkbox absolute z-10 top-2 left-2 bg-base-100/60 border border-base-content/30"
                    />
                    <img
                        v-if="media.type?.includes('image/')"
                        :src="mediaPath(media)"
                        :alt="media.alt ?? media.filename ?? ''"
                        class="w-full h-full object-contain rounded-t"
                    />
                    <i v-else class="bi text-8xl" :class="iconFrom(media.type)"></i>
                    <span
                        class="bg-black/60 text-white/80 hyphens-auto rounded-xs font-bold absolute z-2 left-0 bottom-0 px-1.5 py-0.5 me-1"
                    >
                        {{ media.filename }}
                    </span>
                </figure>
                <div class="card-body bg-base-200 border-t border-t-base-content/20 px-4 pt-2 pb-4">
                    <ul class="list">
                        <li><strong>Type:</strong> {{ mimeType(media) }}</li>
                        <li v-if="media.width"><strong>Dimension:</strong> {{ media.width }}x{{ media.height }}</li>
                        <li v-if="media.size"><strong>Size:</strong> {{ formatBytes(media.size!) }}</li>
                        <li v-if="media.created_at">
                            <strong>Uploaded:</strong> {{ dayjs(media.created_at).format('YYYY-MM-DD HH:mm:ss') }}
                        </li>
                        <li v-if="media.variants">
                            <p><i class="bi bi-collection me-1"></i> {{ variantsExt(media.variants) }}</p>
                            <p><i class="bi bi-aspect-ratio me-1"></i> {{ variantsDim(media.variants) }}</p>
                        </li>
                    </ul>

                    <!-- <div class="card-actions justify-end">action</div> -->
                </div>
            </div>
        </div>
        <GenericModal ref="uploadModal" title="Upload Files" :cancel-action="resetUpload" :ok-action="runUpload">
            <FileUpload ref="uploader" :key="uploadKey" />
        </GenericModal>

        <GenericProgress />

        <GenericModal ref="deleteModal" title="Delete Selection" :ok-action="contentMedia">
            <p>Are you sure you want to delete this file{{ selectCount > 1 ? 's' : '' }}?</p>
        </GenericModal>
    </div>
</template>
<style>
.checker {
    background: repeating-conic-gradient(var(--color-base-100) 0 90deg, var(--color-base-300) 0 180deg) 0 0/40px 40px
        round;
}
</style>
