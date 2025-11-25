<script setup lang="ts">
import { ref, computed } from 'vue'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { formatBytes, mediaPath } from '@/utils/helper'

import GenericPagination from '@/components/GenericPagination.vue'

const auth = useAuth()
const store = useIndex()

const mediaModal = ref()

const medias = ref<Media[]>([])
const apiURL = ref('/api/media')
const total = ref(0)
const limit = ref(8)
const limits = [8, 12]
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

const search = ref('')
const searchVar = computed({
    get() {
        return search.value ? `&search=${search.value}` : ''
    },
    set(newValue) {
        search.value = newValue
    },
})

defineProps({
    update: {
        type: Function,
        default() {
            return {}
        },
    },
})

defineExpose({
    showModal: () => mediaModal.value?.showModal(),
    close: () => mediaModal.value?.close(),
})

function onPageChange() {
    selectMedia()
}

selectMedia()

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
</script>
<template>
    <div>
        <dialog ref="mediaModal" id="media_modal" class="modal items-start">
            <div class="modal-box w-11/12 max-w-5xl mt-20">
                <form method="dialog">
                    <button class="btn btn-sm btn-circle btn-ghost absolute right-2 top-2">
                        <i class="bi bi-x-lg"></i>
                    </button>
                </form>
                <h3 class="text-lg font-bold">Select Media</h3>
                <div class="flex mt-2">
                    <div class="grow">
                        <label class="input">
                            <i class="bi bi-search opacity-45"></i>
                            <input v-model="search" type="search" placeholder="Search" @keyup="selectMedia()" />
                        </label>
                    </div>

                    <GenericPagination
                        v-model:limit="limit"
                        v-model:offset="offset"
                        :hide-stat="true"
                        :total="total"
                        :page-sizes="limits"
                        @change="onPageChange"
                    />
                </div>

                <div class="flex gap-4 flex-wrap justify-start mt-4">
                    <div
                        v-for="media in medias"
                        :key="media.id ?? media.filename!"
                        class="card bg-base-100 w-58 shadow-sm rounded border border-base-content/20 hover:scale-[1.01] hover:shadow-md transition-transform cursor-pointer"
                        @click="update(media)"
                    >
                        <figure class="relative checker h-39">
                            <img
                                :src="mediaPath(media)"
                                :alt="media.alt ?? media.filename ?? ''"
                                class="w-full h-full object-contain rounded-t"
                            />
                            <span
                                class="bg-black/60 text-white/80 hyphens-auto rounded-xs font-bold absolute z-2 left-0 bottom-0 px-1.5 py-0.5 me-1 break-all"
                            >
                                {{ media.filename }}
                            </span>
                        </figure>
                        <div class="card-body bg-base-200 border-t border-t-base-content/20 px-4 pt-2 pb-4">
                            <ul class="list">
                                <li class="break-all"><strong>Alt:</strong> {{ media.alt }}</li>
                                <li v-if="media.width">
                                    <strong>Dimension:</strong> {{ media.width }}x{{ media.height }}
                                </li>
                                <li v-if="media.size"><strong>Size:</strong> {{ formatBytes(media.size!) }}</li>
                            </ul>
                        </div>
                    </div>
                </div>
            </div>
        </dialog>
    </div>
</template>
