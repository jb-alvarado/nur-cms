<script setup lang="ts">
import { ref } from 'vue'
import { cloneDeep } from 'lodash-es'
import { mediaPath } from '@/utils/helper'
import GenericModal from './GenericModal.vue'
import MediaBrowser from './MediaBrowser.vue'

const modal = ref<InstanceType<typeof GenericModal>>()
const keyInput = ref('')
const valueInput = ref('')
const newBlock = ref<Record<string, any>>({})
const mediaModal = ref()
const media = ref<null | Media>(null)

const emit = defineEmits<{
    'add-block': [{ media: null | Media; data: Record<string, any> }]
}>()

const addKeyValue = () => {
    if (keyInput.value.trim() && valueInput.value.trim()) {
        newBlock.value[keyInput.value] = valueInput.value
        keyInput.value = ''
        valueInput.value = ''
    }
}

const removeKeyValue = (key: string) => {
    delete newBlock.value[key]
}

const saveBlock = () => {
    if (keyInput.value && valueInput.value && !newBlock.value[keyInput.value]) {
        addKeyValue()
    }
    emit('add-block', { media: cloneDeep(media.value) ?? null, data: { ...newBlock.value } })
    resetModal()
    modal.value?.close?.()
}

const resetModal = () => {
    newBlock.value = {}
    keyInput.value = ''
    valueInput.value = ''
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
    <GenericModal ref="modal" width="5xl" :title="$t('block.create')" :ok-action="saveBlock">
        <div class="flex flex-col gap-4">
            <div class="flex gap-2">
                <fieldset class="fieldset py-2 min-w-56">
                    <legend class="fieldset-legend">{{ $t('common.key') }}</legend>
                    <input
                        v-model="keyInput"
                        type="text"
                        class="input"
                        :placeholder="$t('common.key')"
                        @keyup.enter="addKeyValue()"
                    />
                </fieldset>
                <fieldset class="fieldset py-2 grow">
                    <legend class="fieldset-legend">{{ $t('common.value') }}</legend>
                    <textarea
                        v-model="valueInput"
                        class="textarea w-full"
                        :placeholder="$t('common.value')"
                        @keyup.enter="addKeyValue()"
                    />
                </fieldset>
                <div class="join mb-0">
                    <button class="btn join-item" @click="addKeyValue()">
                        <i class="bi bi-plus-lg"></i>
                    </button>
                    <button class="btn join-item" @click="openMediaBrowser()">
                        <i class="bi bi-image"></i>
                    </button>
                </div>
            </div>

            <img v-if="media" :src="mediaPath(media)" :alt="media.alt ?? 'Image'" class="object-cover w-18 h-18" />
            <div v-if="Object.keys(newBlock).length > 0" class="bg-base-200 p-3 rounded">
                <div v-for="(value, key) in newBlock" :key="key" class="flex justify-between items-center py-1">
                    <span class="text-sm">
                        <strong>{{ key }}:</strong> {{ value }}
                    </span>
                    <button class="btn btn-xs btn-ghost" @click="removeKeyValue(key as string)">
                        <i class="bi bi-trash"></i>
                    </button>
                </div>
            </div>

            <p v-else class="text-base-content/50 text-sm">{{ $t('block.empty') }}</p>
        </div>
    </GenericModal>
    <MediaBrowser ref="mediaModal" :update="addMedia" />
</template>
