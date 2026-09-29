<script setup lang="ts">
import { computed } from 'vue'
import { useIndex } from '@/stores/index'

const props = defineProps<{ mediaId: number }>()
const store = useIndex()
const progress = computed(() => store.mediaProgress[props.mediaId])
</script>

<template>
    <div v-if="progress" class="mt-2 text-sm" role="status">
        <div class="flex justify-between gap-2">
            <span>{{ $t(`media.progress.${progress.phase}`) }}</span>
            <span>{{ progress.percent }} %</span>
        </div>
        <progress
            class="progress progress-info w-full"
            :value="progress.percent"
            max="100"
            :aria-label="$t('media.processingStatus')"
        ></progress>
    </div>
</template>
