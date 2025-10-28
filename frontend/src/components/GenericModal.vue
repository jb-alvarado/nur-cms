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
    hideButtons: {
        type: Boolean,
        default: false,
    },
})

defineExpose({
    showModal: () => genModal.value?.showModal(),
    close: () => genModal.value?.close(),
})
</script>

<template>
    <dialog ref="genModal" id="generic_modal" class="modal modal-bottom sm:modal-middle">
        <div class="modal-box">
            <h3 class="text-lg font-bold">{{ title }}</h3>

            <div class="py-4">
                <slot />
            </div>
            <div class="modal-action">
                <form method="dialog">
                    <button class="btn" @click="cancelAction()">Cancel</button>
                    <button class="btn" @click="okAction()">Ok</button>
                </form>
            </div>
        </div>
    </dialog>
</template>
