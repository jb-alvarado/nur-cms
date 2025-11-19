<script setup lang="ts">
import { ref } from 'vue'
import dayjs from 'dayjs'
// import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'

import FileUpload from '@/components/FileUpload.vue'
import GenericModal from '@/components/GenericModal.vue'

// const auth = useAuth()
const store = useIndex()
const uploadModal = ref()
const medias = ref<Media[]>([])

const apiURL = ref('/api/media?media_type=image')
const limit = ref(20)
const search = ref('')
const ordering = ref('-created_at')
const previous = ref('')
const next = ref('')

selectMedia()

async function selectMedia(u: string | null = null) {
    const url = u ? u : apiURL.value + `&limit=${limit.value}${search.value}&ordering=${ordering.value}`

    await fetch(url)
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            return resp.json()
        })
        .then(async (res) => {
            if (res.results?.length > 0) {
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

function mediaPath(media: Media) {
    return `${media.path}/${media.filename}`
}

function extension(filename: string) {
    const match = filename.match(/\.([^.]+)$/)
    return match ? match[1]?.toUpperCase() : ''
}
</script>
<template>
    <div>
        <button class="btn btn-primary m-4" @click="uploadModal.showModal()">Upload Media</button>
        <div class="flex gap-4 flex-wrap justify-start p-4">
            <div
                v-for="media in medias"
                :key="media.id ?? media.filename!"
                class="card bg-base-100 w-64 shadow-sm rounded border border-base-content/20 hover:scale-[1.01] transition-transform cursor-pointer"
            >
                <figure class="relative checker h-43">
                    <input v-model="media.check" type="checkbox" class="checkbox absolute z-10 top-2 left-2 bg-base-100/60 border border-base-content/30" />
                    <img
                        :src="mediaPath(media)"
                        :alt="media.alt ?? media.filename ?? ''"
                        class="w-full h-full object-contain rounded-t"
                    />
                    <span
                        class="bg-black/60 text-white/80 hyphens-auto rounded-xs font-bold absolute z-2 left-0 bottom-0 px-2 py-1 me-1"
                        >
                        {{ media.filename }}
                    </span>
                </figure>
                <div class="card-body border-t border-t-base-content/20 px-4 pt-2 pb-4">
                    <ul>
                        <li><strong>Type:</strong> {{ extension(media.filename ?? '') }}</li>
                        <li v-if="media.created_at">
                            <strong>Uploaded:</strong> {{ dayjs(media.created_at).format('YYYY-MM-DD HH:mm:ss') }}
                        </li>
                    </ul>

                    <!-- <div class="card-actions justify-end">action</div> -->
                </div>
            </div>
        </div>
        <GenericModal ref="uploadModal" title="Upload Media" :hide-cancel="true" :ok-action="selectMedia">
            <FileUpload />
        </GenericModal>
    </div>
</template>
<style>
.checker {
    background: repeating-conic-gradient(var(--color-base-100) 0 90deg, var(--color-base-300) 0 180deg) 0 0/40px 40px
        round;
}
</style>
