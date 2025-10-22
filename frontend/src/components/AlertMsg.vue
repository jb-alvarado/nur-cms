<script setup lang="ts">
import { useIndex } from '@/stores/index'

const indexStore = useIndex()
</script>

<template>
    <div
        v-if="indexStore.showAlert"
        role="alert"
        class="alert fixed top-20 right-3 z-40 w-auto min-w-64 max-w-[90%] md:max-w-[50%] xl:max-w-[40%] text-black!"
        :class="`alert-${indexStore.alertVariant}`"
    >
        <i v-if="indexStore.alertVariant === 'error'" class="bi bi-exclamation-triangle-fill text-2xl"></i>
        <i v-else-if="indexStore.alertVariant === 'warning'" class="bi bi-x-circle text-2xl"></i>
        <i v-else-if="indexStore.alertVariant === 'info'" class="bi bi-check-circle text-2xl"></i>
        {{ indexStore.alertMsg }}
    </div>
</template>
