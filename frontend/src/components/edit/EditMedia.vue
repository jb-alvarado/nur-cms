<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import { useIndex } from '@/stores/index'
import { mediaPath } from '@/utils/helper'
import { authFetch } from '@/composables/authFetch'

const { t } = useI18n()
const store = useIndex()
const media = ref<Media>({})
const mediaOriginal = ref<Media>({})

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
            <div v-if="media.type?.startsWith('video/')" class="mt-3 flex items-center gap-2 text-sm">
                <span>{{ $t(`media.processing.${media.processing_status ?? 'completed'}`) }}</span>
                <button
                    v-if="media.processing_status === 'failed'"
                    type="button"
                    class="btn btn-sm"
                    @click="retryVideo"
                >
                    {{ $t('media.retryVideo') }}
                </button>
            </div>
        </div>
    </div>
</template>
