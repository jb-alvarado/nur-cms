<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import { useIndex } from '@/stores/index'
import { mediaPath } from '@/utils/helper'
import { authFetch } from '@/composables/authFetch'

import MediaBrowser from '@/components/media/MediaBrowser.vue'

const { t } = useI18n()
const store = useIndex()
const media = ref<Media>({})
const mediaOriginal = ref<Media>({})
const thumbnailModal = ref()
const thumbnailQueued = ref(false)

const props = defineProps({
    id: {
        type: Number,
        default: 0,
    },
})

defineExpose({
    async update() {
        await updateMedia()
    },
})

selectMedia()

onMounted(() => window.addEventListener('nur-cms:media-variants-ready', refreshAfterThumbnail))
onBeforeUnmount(() => window.removeEventListener('nur-cms:media-variants-ready', refreshAfterThumbnail))

async function refreshAfterThumbnail(event: Event) {
    const detail = event instanceof CustomEvent ? (event.detail as { mediaId?: unknown }) : undefined
    if (!thumbnailQueued.value || detail?.mediaId !== props.id) return

    await selectMedia()
    thumbnailQueued.value = ['queued', 'processing'].includes(media.value.processing_status ?? '')
}

async function selectMedia() {
    const url = `/api/media?id=${props.id}`

    await authFetch<RespondObj>(url)
        .then(async (res) => {
            if (res.results?.length > 0) {
                media.value = res.results[0]
                mediaOriginal.value = cloneDeep(res.results[0])
            } else {
                media.value = {}
            }
        })
        .catch((err) => {
            store.msgAlert('error', err)
        })
}

async function retryVideo() {
    await authFetch(`/api/media/${props.id}/retry-video`, { method: 'POST' })
        .then(() => {
            media.value.processing_status = 'queued'
            store.msgAlert('success', t('media.videoRetryQueued'))
        })
        .catch((err) => {
            store.msgAlert('error', err)
        })
}

function openThumbnailBrowser() {
    thumbnailModal.value?.showModal()
}

async function replaceThumbnail(thumbnail: Media) {
    if (!thumbnail.id) return

    await authFetch(`/api/media/${props.id}/thumbnail`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ media_id: thumbnail.id }),
    })
        .then(() => {
            thumbnailQueued.value = true
            media.value.processing_status = 'queued'
            thumbnailModal.value?.close()
            store.msgAlert('success', t('media.thumbnailQueued'))
        })
        .catch((err) => store.msgAlert('error', err))
}

async function regenerateThumbnail() {
    await authFetch(`/api/media/${props.id}/regenerate-thumbnail`, { method: 'POST' })
        .then(() => {
            thumbnailQueued.value = true
            media.value.processing_status = 'queued'
            store.msgAlert('success', t('media.thumbnailQueued'))
        })
        .catch((err) => store.msgAlert('error', err))
}

async function updateMedia() {
    const url = `/api/media/${props.id}`

    const payload = Object.fromEntries(
        Object.entries(media.value).filter(([key, value]) => {
            return (
                ['alt', 'filename'].includes(key) &&
                !isEqual(value, mediaOriginal.value[key as keyof Media])
            )
        }),
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', t('media.noChanges'))
        return
    }

    await authFetch(url, {
        method: 'PUT',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(() => {
            store.msgAlert('success', t('media.updateSuccess', { id: props.id }))
        })
        .catch((err) => {
            store.msgAlert('error', err)
        })
}
</script>
<template>
    <div class="flex gap-4">
        <div class="mt-3">
            <img v-if="media.type?.startsWith('image/')" :src="mediaPath(media)" :alt="media.alt ?? ''" width="210" />
            <video
                v-else-if="media.type?.startsWith('video/')"
                :src="mediaPath(media)"
                controls
                preload="metadata"
                width="210"
            />
            <i v-else class="bi bi-file-earmark text-8xl"></i>
        </div>
        <div class="grow">
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('media.altText') }}</legend>
                <input v-model="media.alt" type="text" class="input w-full" :placeholder="$t('media.alt')" />
            </fieldset>
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('media.filename') }}</legend>
                <input v-model="media.filename" type="text" class="input w-full" :placeholder="$t('media.filename')" />
            </fieldset>
            <div v-if="media.type?.startsWith('video/')" class="mt-3 flex flex-wrap items-center gap-2 text-sm">
                <span class="me-auto">{{ $t(`media.processing.${media.processing_status ?? 'completed'}`) }}</span>
                <button
                    v-if="media.processing_status === 'failed'"
                    type="button"
                    class="btn btn-sm"
                    @click="retryVideo"
                >
                    {{ $t('media.retryVideo') }}
                </button>
                <button
                    type="button"
                    class="btn btn-sm"
                    :disabled="thumbnailQueued || ['queued', 'processing'].includes(media.processing_status ?? '')"
                    @click="openThumbnailBrowser"
                >
                    {{ $t('media.replaceThumbnail') }}
                </button>
                <button
                    type="button"
                    class="btn btn-sm"
                    :disabled="thumbnailQueued || ['queued', 'processing'].includes(media.processing_status ?? '')"
                    @click="regenerateThumbnail"
                >
                    {{ $t('media.regenerateThumbnail') }}
                </button>
            </div>
        </div>
    </div>
    <MediaBrowser ref="thumbnailModal" :media-types="['image']" :update="replaceThumbnail" />
</template>
