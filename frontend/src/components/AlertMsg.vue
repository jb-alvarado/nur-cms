<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { storeToRefs } from 'pinia'
import { useIndex } from '@/stores/index'

const indexStore = useIndex()

const progressContainer = ref()
const { msgList } = storeToRefs(useIndex())

watch(msgList.value, () => {
    nextTick(() => {
        if (progressContainer.value) {
            progressContainer.value.scrollTop = progressContainer.value.scrollHeight + 50
        }
    })
})
</script>

<template>
    <div
        ref="progressContainer"
        class="toast toast-end fixed top-12 z-40 h-auto! max-h-[80%] overflow-y-auto gap-1"
        :style="`height: ${indexStore.msgList.length * 80}px`"
    >
        <div
            v-for="msg in indexStore.msgList"
            :key="msg.text"
            role="alert"
            class="alert w-auto max-w-[800px] justify-start py-2 rounded-sm text-black!"
            :class="`alert-${msg.variance}`"
        >
            <i v-if="msg.variance === 'error'" class="bi bi-exclamation-triangle-fill text-2xl"></i>
            <i v-else-if="msg.variance === 'warning'" class="bi bi-x-circle text-2xl"></i>
            <i v-else-if="msg.variance === 'info'" class="bi bi-info-circle text-2xl"></i>
            <i v-else-if="msg.variance === 'success'" class="bi bi-check-circle text-2xl"></i>
            {{ msg.text }}
        </div>
    </div>
</template>
