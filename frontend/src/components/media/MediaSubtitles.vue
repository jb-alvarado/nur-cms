<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { authFetch } from '@/composables/authFetch'
import { useIndex } from '@/stores/index'
import type { MediaSubtitle, SubtitleUpload } from '@/types/subtitles.d'

const props = defineProps<{ mediaId: number; disabled: boolean }>()
const emit = defineEmits<{ changed: [] }>()
const { t } = useI18n()
const store = useIndex()
const tracks = ref<MediaSubtitle[]>([])
const language = ref('de')
const label = ref('Deutsch')
const isDefault = ref(false)
const fileInput = ref<HTMLInputElement>()
const busy = ref(false)

onMounted(async () => {
    try {
        tracks.value = await authFetch<MediaSubtitle[]>(`/api/media/${props.mediaId}/subtitles`)
    } catch (error) {
        store.msgAlert('error', error instanceof Error ? error.message : String(error))
    }
})

async function upload() {
    const file = fileInput.value?.files?.[0]
    if (!file) return
    if (!file.name.toLowerCase().endsWith('.vtt') || file.size > 512 * 1024) {
        store.msgAlert('error', t('media.subtitles.invalidFile'))
        return
    }
    busy.value = true
    try {
        const webvtt = new TextDecoder('utf-8', { fatal: true }).decode(await file.arrayBuffer())
        const payload: SubtitleUpload = {
            language: language.value,
            label: label.value,
            is_default: isDefault.value,
            webvtt,
        }
        tracks.value = await authFetch<MediaSubtitle[]>(`/api/media/${props.mediaId}/subtitles`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        })
        if (fileInput.value) fileInput.value.value = ''
        store.msgAlert('success', t('media.subtitles.saved'))
        emit('changed')
    } catch (error) {
        store.msgAlert('error', error instanceof Error ? error.message : String(error))
    } finally {
        busy.value = false
    }
}

async function remove(track: MediaSubtitle) {
    busy.value = true
    try {
        tracks.value = await authFetch<MediaSubtitle[]>(`/api/media/${props.mediaId}/subtitles/${track.id}`, {
            method: 'DELETE',
        })
        store.msgAlert('success', t('media.subtitles.removed'))
        emit('changed')
    } catch (error) {
        store.msgAlert('error', error instanceof Error ? error.message : String(error))
    } finally {
        busy.value = false
    }
}
</script>

<template>
    <section class="mt-4 border-t border-base-content/20 pt-3">
        <h3 class="font-semibold">{{ t('media.subtitles.title') }}</h3>
        <p class="mt-1 text-xs opacity-70">{{ t('media.subtitles.hint') }}</p>
        <ul class="my-2 space-y-2">
            <li v-for="track in tracks" :key="track.id" class="flex items-center gap-2 text-sm">
                <span class="grow"
                    >{{ track.label }} ({{ track.language }})
                    <span v-if="track.is_default" class="badge badge-sm">{{ t('media.subtitles.default') }}</span>
                </span>
                <button type="button" class="btn btn-sm" :disabled="disabled || busy" @click="remove(track)">
                    {{ t('media.subtitles.remove') }}
                </button>
            </li>
        </ul>
        <form @submit.prevent="upload">
            <fieldset :disabled="disabled || busy" class="fieldset gap-2">
                <label class="label" for="subtitle-language">{{ t('media.subtitles.language') }}</label>
                <input
                    id="subtitle-language"
                    v-model="language"
                    required
                    maxlength="35"
                    placeholder="de-DE"
                    class="input w-full"
                />
                <label class="label" for="subtitle-label">{{ t('media.subtitles.label') }}</label>
                <input id="subtitle-label" v-model="label" required maxlength="100" class="input w-full" />
                <label class="label" for="subtitle-file">{{ t('media.subtitles.file') }}</label>
                <input
                    id="subtitle-file"
                    ref="fileInput"
                    type="file"
                    accept=".vtt,text/vtt"
                    required
                    class="file-input w-full"
                />
                <label class="label"
                    ><input v-model="isDefault" type="checkbox" class="checkbox checkbox-sm" />{{
                        t('media.subtitles.default')
                    }}</label
                >
                <button type="submit" class="btn btn-sm">{{ t('media.subtitles.upload') }}</button>
            </fieldset>
        </form>
    </section>
</template>
