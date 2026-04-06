<script setup lang="ts">
import { computed, ref, watch, nextTick } from 'vue'
import dayjs from 'dayjs'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { formatBytes, shortID, mediaPath, iconFrom } from '@/utils/helper'

import FileUpload from '@/components/media/FileUpload.vue'
import GenericModal from '@/components/generic/GenericModal.vue'
import GenericPagination from '@/components/generic/GenericPagination.vue'
import GenericProgress from '@/components/generic/GenericProgress.vue'
import EditMedia from '@/components/edit/EditMedia.vue'

const auth = useAuth()
const store = useIndex()
const deleteModal = ref()
const uploadModal = ref()
const editModal = ref()
const uploader = ref()
const updater = ref()
const medias = ref<Media[]>([])
const selectCount = computed(() => medias.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const uploadKey = ref(shortID())

const apiURL = ref('/api/media')
const editID = ref(0)
const total = ref(0)
const limit = ref(12)
const ordering = ref('-created_at')
const offset = ref(0)
const offsetVar = computed({
    get() {
        return offset.value > 0 ? `&offset=${offset.value}` : ''
    },
    set(newValue) {
        offset.value = Number(newValue)
    },
})
const limits = [12, 24, 50]
const search = ref('')
const searchVar = computed({
    get() {
        return search.value ? `&search=${search.value}` : ''
    },
    set(newValue) {
        search.value = newValue
    },
})

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

const openUpdateModal = (id: number) => {
    editID.value = id
    nextTick(() => {
        editModal.value.showModal()
    })
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
            } else {
                medias.value = []
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
    const dims = new Map<number, string>()

    for (const v of variants) {
        if (!dims.has(v.width)) {
            dims.set(v.width, `${v.width}x${v.height}`)
        }
    }

    return Array.from(dims.entries())
        .sort(([w1], [w2]) => w1 - w2)
        .map(([, dim]) => dim)
        .join(' | ')
}

function variantsExt(variants: Variants[]) {
    const exts = new Set<string>()

    for (const v of variants) {
        const ext = extension(v.filename)
        if (!ext) continue
        exts.add(ext)
    }

    return Array.from(exts).sort().join(' | ')
}

function runUpload() {
    if (uploader.value) {
        uploader.value.upload()
    }
}

async function runUpdate() {
    if (updater.value) {
        await updater.value.update()

        selectMedia()
    }
}

function resetUpload() {
    uploadKey.value = shortID()
}
</script>
<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ $t('button.media') }}</h1>
            <button class="btn btn-sm btn-primary text-base" @click="uploadModal.showModal()">
                {{ $t('media.upload') }}
            </button>
        </div>

        <div class="h-10 mt-4 mb-6 flex items-center">
            <div class="grow join">
                <label class="input" :class="selectCount > 0 ? 'w-40' : 'w-74'">
                    <i class="bi bi-search opacity-45"></i>
                    <input v-model="search" type="search" :placeholder="$t('common.search')" @keyup="selectMedia()" />
                </label>
                <div v-if="selectCount > 0">
                    <button class="btn text-warning join-item" @click="openDeleteModal">
                        {{ $t('common.delete') }}
                    </button>
                    <span class="ms-2">{{ selectCount }} {{ $t('common.selected') }}</span>
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
                class="card bg-base-100 w-64 shadow-sm rounded border border-base-content/20 hover:scale-[1.01] hover:shadow-md transition-transform"
            >
                <figure class="relative bg-checker h-43">
                    <input
                        v-model="media.check"
                        type="checkbox"
                        class="checkbox absolute z-10 top-2 left-2 bg-base-100/60 border border-base-content/30"
                    />
                    <img
                        v-if="media.type?.includes('image/')"
                        :src="mediaPath(media)"
                        :alt="media.alt ?? media.filename ?? ''"
                        class="w-full h-full object-contain rounded-t cursor-pointer"
                        @click="openUpdateModal(media.id!)"
                    />
                    <i v-else class="bi text-8xl" :class="iconFrom(media.type)"></i>
                    <button
                        class="bg-black/60 text-white/80 hyphens-auto rounded-xs font-bold text-left absolute z-2 left-0 bottom-0 px-1.5 py-0.5 me-1 break-all cursor-pointer"
                        @click="openUpdateModal(media.id!)"
                    >
                        {{ media.filename }}
                    </button>
                </figure>
                <div
                    class="card-body bg-base-200 border-t border-t-base-content/20 px-4 pt-2 pb-4 cursor-pointer"
                    @click="openUpdateModal(media.id!)"
                >
                    <ul class="list">
                        <li class="break-all">
                            <strong>{{ $t('media.alt') }}:</strong> {{ media.alt }}
                        </li>
                        <li>
                            <strong>{{ $t('media.type') }}:</strong> {{ mimeType(media) }}
                        </li>
                        <li v-if="media.width">
                            <strong>{{ $t('media.dimension') }}:</strong> {{ media.width }}x{{ media.height }}
                        </li>
                        <li v-if="media.size">
                            <strong>{{ $t('media.size') }}:</strong> {{ formatBytes(media.size!) }}
                        </li>
                        <li v-if="media.created_at">
                            <strong>{{ $t('media.uploaded') }}:</strong>
                            {{ dayjs(media.created_at).format('YYYY-MM-DD HH:mm:ss') }}
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
        <GenericModal
            ref="uploadModal"
            :title="$t('media.uploadFiles')"
            :cancel-action="resetUpload"
            :ok-action="runUpload"
        >
            <FileUpload ref="uploader" :key="uploadKey" />
        </GenericModal>

        <GenericProgress />

        <GenericModal ref="deleteModal" :title="$t('dialog.deleteTitle')" :ok-action="contentMedia">
            <p>{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
        </GenericModal>

        <GenericModal ref="editModal" :key="editID" :title="$t('media.editTitle')" width="2xl" :ok-action="runUpdate">
            <EditMedia ref="updater" :id="editID" />
        </GenericModal>
    </div>
</template>
