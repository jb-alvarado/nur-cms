<script setup lang="ts">
import { useIndex } from '@/stores/index'
const store = useIndex()

function close() {
    store.progressShow = false
}

</script>

<template>
    <div v-if="store.progressShow" class="absolute left-0 bottom-6 z-20 w-full flex justify-center px-8 pointer-events-none">
        <div role="alert" class="alert md:min-w-[400px] alert-vertical sm:alert-horizontal shadow/30 pointer-events-auto">
            <i class="bi bi-info-circle text-info text-xl leading-0"></i>
            <div class="w-full">
                <progress class="progress progress-info w-full" :value="store.progress" max="100"></progress>
            </div>
            <div>
                <button class="btn btn-sm p-2" @click="close()">
                    <i class="bi bi-x-lg"></i>
                </button>
            </div>
        </div>
    </div>
</template>
