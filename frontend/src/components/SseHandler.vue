<script setup lang="ts">
import { ref, onBeforeUnmount, watch } from 'vue'
import { useEventSource } from '@vueuse/core'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores'
import { filenameFromProcessingMessage } from '@/utils/mediaProcessing'
import { completeMediaProgress, resetMediaProgress } from '@/utils/mediaProgress'

const auth = useAuth()
const store = useIndex()

const streamUrl = ref(`/sse?uuid=${auth.uuid ?? ''}`)
const sseConnected = ref(false)
const errorCounter = ref(0)

const { status, data, error, close } = useEventSource(streamUrl, [], {
    autoReconnect: {
        retries: -1,
        delay: 1000,
        onFailed() {
            sseConnected.value = false
        },
    },
})

onBeforeUnmount(() => {
    close()
    resetMediaProgress(store.mediaProgress)
    sseConnected.value = false
})

watch([status, error], async () => {
    if (status.value === 'OPEN') {
        sseConnected.value = true
        errorCounter.value = 0
    } else {
        resetMediaProgress(store.mediaProgress)
        // Allow an identical heartbeat after reconnect to trigger the data watcher.
        data.value = null
        sseConnected.value = false
        errorCounter.value += 1

        if (errorCounter.value > 15) {
            await auth.obtainUuid()
            streamUrl.value = `/sse?uuid=${auth.uuid ?? ''}`
            errorCounter.value = 0
        }
    }
})

watch([data], () => {
    if (data.value) {
        try {
            const msg = JSON.parse(data.value) as SSEMessage
            if (msg.progress && msg.media_id !== undefined) {
                store.setMediaProgress(msg.media_id, msg.progress)
                return
            }
            store.msgAlert(msg.variance, msg.text)
            const mediaFilename = filenameFromProcessingMessage(msg.text)
            if (mediaFilename) {
                const mediaId = msg.media_id
                if (mediaId !== undefined && store.mediaProgress[mediaId]) {
                    if (msg.variance === 'success') {
                        completeMediaProgress(store.mediaProgress, mediaId)
                    } else {
                        store.clearMediaProgress(mediaId)
                    }
                }
                window.dispatchEvent(
                    new CustomEvent('nur-cms:media-variants-ready', {
                        detail: { filename: mediaFilename, mediaId: msg.media_id, message: msg.text },
                    }),
                )
            }
        } catch {
            store.msgAlert('error', data.value)
            sseConnected.value = true
        }
    }
})
</script>
<template><div></div></template>
