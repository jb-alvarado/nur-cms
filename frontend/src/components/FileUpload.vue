<script setup lang="ts">
import { ref } from 'vue'
import { errMsg } from '@/utils/error'
import { formatBytes } from '@/utils/helper'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { shortID } from '@/utils/helper'

const auth = useAuth()
const store = useIndex()

const uploading = ref(false)
const progress = ref(0)
const uploadSpeed = ref('0kb/s')
const error = ref('')

const MAX_PARALLEL_UPLOADS = 4
const DEFAULT_CHUNK_SIZE = 1024 * 512 // 512 kb

const speedHistory: number[] = []
const MAX_HISTORY = 5
let lastLoaded = 0
const lastTime = ref(Date.now())

function updateProgress(completedChunks: number, fileSize: number, currentIndex: number, batchCount: number) {
    const now = Date.now()
    const loadedBytes = Math.min(completedChunks * DEFAULT_CHUNK_SIZE, fileSize)
    const deltaBytes = loadedBytes - lastLoaded
    const deltaTime = (now - lastTime.value) / 1000

    const instantSpeed = deltaBytes / deltaTime
    speedHistory.push(instantSpeed)
    if (speedHistory.length > MAX_HISTORY) speedHistory.shift()

    const avgSpeed = speedHistory.reduce((a, b) => a + b, 0) / speedHistory.length

    // Calculate progress considering current file in batch
    const currentFileProgress = loadedBytes / fileSize
    const totalProgress = (currentIndex  + currentFileProgress) / batchCount

    progress.value = Math.round(totalProgress * 100)
    uploadSpeed.value = formatBytes(avgSpeed) + '/s'
    lastLoaded = loadedBytes
    lastTime.value = now
}

async function uploadFile(file: File, batch_id: string, currentIndex: number, count: number, chunkSize = DEFAULT_CHUNK_SIZE) {
    let offset = 0
    const totalChunks = Math.ceil(file.size / chunkSize)
    const fileSize = file.size
    let completedChunks = 0

    const queue: { start: number; end: number; blob: Blob }[] = []

    while (offset < file.size) {
        const end = Math.min(offset + chunkSize, file.size)
        queue.push({ start: offset, end, blob: file.slice(offset, end) })
        offset = end
    }

    async function worker() {
        while (queue.length) {
            const { start, end, blob } = queue.shift()!
            const form = new FormData()
            form.append('fileName', file.name)
            form.append('start', start.toString())
            form.append('end', end.toString())
            form.append('size', fileSize.toString())
            form.append('chunk', blob)
            form.append('batch_id', batch_id)
            form.append('batch_count', count.toString())

            const resp = await fetch('/api/upload', {
                method: 'POST',
                headers: auth.authHeader,
                body: form,
            })

            if (!resp.ok) {
                const err = await errMsg(resp)
                throw new Error(err)
            }

            completedChunks++
            updateProgress(completedChunks, fileSize, currentIndex, count)
        }
    }

    const workers = Array(Math.min(MAX_PARALLEL_UPLOADS, totalChunks))
        .fill(0)
        .map(() => worker())

    await Promise.all(workers)
}

async function onFileChange(e: Event) {
    const input = e.target as HTMLInputElement
    if (!input.files?.length) return
    const length = input.files.length

    uploading.value = true
    progress.value = 0
    error.value = ''

    try {
        if (length > 0) {
            const id = shortID()

            for (const [i, file] of Array.from(input.files).entries()) {
                await uploadFile(file, id, i, length)
            }

            progress.value = 100
            store.msgAlert('success', 'Upload complete!')
        }
    } catch (err: any) {
        store.msgAlert('error', err)
        error.value = err.message || 'Upload failed'
    } finally {
        uploading.value = false
    }
}
</script>

<template>
    <div>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">Pick a file</legend>
            <input type="file" class="file-input" @change="onFileChange" multiple />
        </fieldset>

        <div>
            <p>
                Uploading: {{ progress }}%
                {{ progress && progress < 100 ? uploadSpeed : '' }}
            </p>
            <progress class="progress progress-warning w-96" :value="progress" max="100"></progress>
        </div>

        <p v-if="error" style="color: red">{{ error }}</p>
    </div>
</template>
