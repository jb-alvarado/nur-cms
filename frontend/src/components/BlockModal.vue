<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { cloneDeep } from 'lodash-es'
import { mediaPath } from '@/utils/helper'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

import GenericModal from './GenericModal.vue'
import MediaBrowser from './MediaBrowser.vue'

const auth = useAuth()
const store = useIndex()

const modal = ref<InstanceType<typeof GenericModal>>()
const mediaModal = ref()
const media = ref<null | Media>(null)
const templates = ref<NodeTemplateExt[]>([])
const selectedTemplate = ref()

const emit = defineEmits<{
    'add-block': [{ media: null | Media; data: Record<string, any> }]
}>()

const selectTemplates = async () => {
    try {
        const response: RespondObj = await fetch('/api/content/node/templates?ordering=id', {
            headers: auth.authHeader,
        })

        if (!response.ok) {
            throw new Error(`API error: ${response.status}`)
        }

        const resp = await response.json()

        templates.value = resp.results
    } catch (err) {
        store.msgAlert('error', `Error fetching templates: ${err}`)
    }
}

onMounted(async () => {
    await selectTemplates()
})

const saveBlock = () => {
    emit('add-block', { media: cloneDeep(media.value) ?? null, data: selectedTemplate.value.data })
    resetModal()
    modal.value?.close?.()
}

const resetModal = () => {
    selectedTemplate.value = {}
    media.value = null
}

const showModal = () => {
    resetModal()
    modal.value?.showModal?.()
}

defineExpose({ showModal })

const openMediaBrowser = () => {
    mediaModal.value.showModal()
}

function addMedia(m: Media) {
    media.value = m

    mediaModal.value.close()
}
</script>

<template>
    <GenericModal ref="modal" :title="$t('block.create')" :ok-action="saveBlock">
        <div class="flex flex-col gap-4">
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('nodeTemplates.title') }}</legend>

                <div class="flex gap-2">
                    <select v-model="selectedTemplate" class="select grow">
                        <option v-for="temp in templates" :key="temp.id" :value="temp">{{ temp.name }}</option>
                    </select>
                    <button class="btn border border-base-content/20" @click="openMediaBrowser()">
                        <i class="bi bi-image"></i>
                    </button>
                </div>
            </fieldset>

            <img v-if="media" :src="mediaPath(media)" :alt="media.alt ?? 'Image'" class="object-cover w-18 h-18" />
        </div>
    </GenericModal>
    <MediaBrowser ref="mediaModal" :update="addMedia" />
</template>
