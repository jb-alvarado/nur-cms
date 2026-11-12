<script setup lang="ts">
import { onBeforeUnmount, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import type { HlsVideoElement } from '@videojs/html/media/hls-video'
import '@/utils/videoPlayer'

defineProps<{ src: string; hls: boolean; poster?: string }>()
const { locale } = useI18n()
const media = ref<HTMLVideoElement | HlsVideoElement>()

function pause() {
    media.value?.pause()
}

defineExpose({ pause })
onBeforeUnmount(pause)
</script>

<template>
    <div class="w-full">
        <media-i18n :lang="locale">
            <video-player :poster="poster">
                <video-skin class="w-full aspect-video">
                    <hls-video v-if="hls" ref="media" :src="src" preload="metadata" playsinline />
                    <video v-else ref="media" :src="src" preload="metadata" playsinline />
                </video-skin>
            </video-player>
        </media-i18n>
    </div>
</template>
