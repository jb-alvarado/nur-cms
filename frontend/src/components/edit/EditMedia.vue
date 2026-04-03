<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { cloneDeep } from 'es-toolkit/object'
import { isEqual } from 'es-toolkit/predicate'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { mediaPath } from '@/utils/helper'

const { t } = useI18n()
const auth = useAuth()
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

async function updateMedia() {
    const url = `/api/media/${props.id}`

    const payload = Object.fromEntries(
        Object.entries(media.value).filter(([key, value]) => {
            return !isEqual(value, mediaOriginal.value[key as keyof Media])
        })
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', t('media.noChanges'))
        return
    }

    await fetch(url, {
        method: 'PUT',
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
            <img :src="mediaPath(media)" :alt="media.alt ?? ''" width="210" />
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
        </div>
    </div>
</template>
