<script setup lang="ts">
import { ref, onBeforeUnmount, watch } from 'vue'
import { useEventSource } from '@vueuse/core'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores'

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
    sseConnected.value = false
})

watch([status, error], async () => {
    if (status.value === 'OPEN') {
        sseConnected.value = true
        errorCounter.value = 0
    } else {
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
            store.msgAlert(msg.variance, msg.text)
        } catch {
            store.msgAlert('error', data.value)
            sseConnected.value = true
        }
    }
})
</script>
<template><div></div></template>
