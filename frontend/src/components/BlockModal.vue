<script setup lang="ts">
import { ref } from 'vue'
import GenericModal from './GenericModal.vue'

const modal = ref<InstanceType<typeof GenericModal>>()
const keyInput = ref('')
const valueInput = ref('')
const newBlock = ref<Record<string, any>>({})

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
    emit('add-block', { ...newBlock.value })
    resetModal()
    modal.value?.close?.()
}

const resetModal = () => {
    newBlock.value = {}
    keyInput.value = ''
    valueInput.value = ''
}

const showModal = () => {
    resetModal()
    modal.value?.showModal?.()
}

defineExpose({ showModal })
const emit = defineEmits<{
    'add-block': [block: Record<string, any>]
}>()
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
                <button class="btn mb-0" @click="addKeyValue()">
                    <i class="bi bi-plus-lg"></i>
                </button>
            </div>

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
</template>
