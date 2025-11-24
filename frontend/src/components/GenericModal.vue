<script setup lang="ts">
import { ref } from 'vue'

const genModal = ref()

defineProps({
    title: {
        type: String,
        default: '',
    },
    okAction: {
        type: Function,
        default() {
            return ''
        },
    },
    cancelAction: {
        type: Function,
        default() {
            return ''
        },
    },
    hideCancel: {
        type: Boolean,
        default: false,
    },
    width: {
        type: String,
        default: ''
    },
})

defineExpose({
    showModal: () => genModal.value?.showModal(),
    close: () => genModal.value?.close(),
})
</script>

<template>
    <dialog ref="genModal" id="generic_modal" class="modal modal-bottom sm:modal-middle">
        <div class="modal-box" :class="width ? `max-w-${width}` : ''">
            <h3 class="text-lg font-bold">{{ title }}</h3>

            <div class="py-2">
                <slot />
            </div>
            <div class="modal-action">
                <form method="dialog">
                    <div class="join">
                        <button v-if="!hideCancel" class="btn join-item" @click="cancelAction()">Cancel</button>
                        <button class="btn join-item" @click="okAction()">Ok</button>
                    </div>
                </form>
            </div>
        </div>
    </dialog>
</template>
