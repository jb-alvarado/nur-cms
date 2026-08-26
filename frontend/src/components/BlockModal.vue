<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { cloneDeep } from 'es-toolkit/object'
import { mediaPath } from '@/utils/helper'
import { useIndex } from '@/stores/index'
import { authFetch } from '@/composables/authFetch'
import { useI18n } from 'vue-i18n'
import type { ContentNodeTemplate } from '@/types/models.d'
import type { RespondObj } from '@/types/query.d'

import GenericModal from '@/components/generic/GenericModal.vue'
import MediaBrowser from '@/components/media/MediaBrowser.vue'

const store = useIndex()
const { t } = useI18n()

type NodeTemplateView = {
    id: number
    name: string
    data: unknown
    schema: Array<{ key: string; default: unknown }>
}

const modal = ref<InstanceType<typeof GenericModal>>()
const mediaModal = ref()
const media = ref<null | Media>(null)
const templates = ref<NodeTemplateView[]>([])
const selectedTemplate = ref<NodeTemplateView | null>(null)

const emit = defineEmits(['add-block', 'template-count'])

const selectTemplates = async () => {
    try {
        const response = await authFetch<RespondObj<ContentNodeTemplate>>('/api/content/node/templates?ordering=id')
        emit('template-count', response.count)
        templates.value = response.results.map((template) => ({
            id: template.id,
            name: template.name,
            data: template.data as unknown,
            schema: template.schema.map((field) => ({
                key: field.key,
                default: field.default as unknown,
            })),
        }))
    } catch (err) {
        store.msgAlert('error', `Error fetching templates: ${err}`)
    }
}

onMounted(async () => {
    await selectTemplates()
})

const saveBlock = () => {
    const selected = selectedTemplate.value
    if (!selected) {
        store.msgAlert('error', t('nodeTemplates.select'))
        return
    }

    const schema = selected.schema ?? []
    const data =
        schema.length > 0
            ? Object.fromEntries(schema.map((field) => [field.key, cloneDeep(field.default)]))
            : cloneDeep(selected.data)

    emit(
        'add-block',
        cloneDeep({
            name: selected.name,
            template_id: selected.id,
            media: media.value ?? null,
            data,
        }),
    )
    resetModal()
    modal.value?.close?.()
}

const resetModal = () => {
    selectedTemplate.value = templates.value[0] ?? null
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
                        <option :value="null" disabled>{{ $t('nodeTemplates.title') }}</option>
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
    <MediaBrowser ref="mediaModal" :update="addMedia" :media-types="['image']" />
</template>
