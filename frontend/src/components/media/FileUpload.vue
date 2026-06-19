<script setup lang="ts">
import { ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errMsg } from '@/utils/error'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { shortID } from '@/utils/helper'

const { t } = useI18n()
const auth = useAuth()
const store = useIndex()

const uploading = ref(false)
const input = ref()
const error = ref('')
const batchId = ref(shortID())
const completedFiles = new Set<string>()

const MAX_PARALLEL_UPLOADS = 4
const DEFAULT_CHUNK_SIZE = 1024 * 512 // 512 kb

const speedHistory: number[] = []
const MAX_HISTORY = 5
let lastLoaded = 0
const lastTime = ref(Date.now())

defineExpose({
    upload() {
        return runJob()
    },
})

function updateProgress(completedChunks: number, fileSize: number, currentIndex: number, batchCount: number) {
    const now = Date.now()
    const loadedBytes = Math.min(completedChunks * DEFAULT_CHUNK_SIZE, fileSize)
    const deltaBytes = loadedBytes - lastLoaded
    const deltaTime = (now - lastTime.value) / 1000

    const instantSpeed = deltaBytes / deltaTime
    speedHistory.push(instantSpeed)
    if (speedHistory.length > MAX_HISTORY) speedHistory.shift()

    // Calculate progress considering current file in batch
    const currentFileProgress = loadedBytes / fileSize
    const totalProgress = (currentIndex + currentFileProgress) / batchCount

    store.progress = Math.round(totalProgress * 100)
    lastLoaded = loadedBytes
    lastTime.value = now
}

async function uploadFile(
    file: File,
    batch_id: string,
    currentIndex: number,
    count: number,
    chunkSize = DEFAULT_CHUNK_SIZE,
) {
    let offset = 0
    const totalChunks = Math.ceil(file.size / chunkSize)
    const fileSize = file.size
    let hasError = false

    const statusParams = new URLSearchParams({
        file_name: file.name,
        size: fileSize.toString(),
        batch_id,
    })
    const statusResponse = await fetch(`/api/upload?${statusParams}`, {
        headers: auth.authHeader,
    })
    if (!statusResponse.ok) {
        throw new Error(await errMsg(statusResponse))
    }

    const status = (await statusResponse.json()) as {
        received_ranges: [number, number][]
        complete: boolean
    }
    if (status.complete) return

    const queue: { start: number; end: number; blob: Blob }[] = []
    lastLoaded = 0
    lastTime.value = Date.now()

    while (offset < file.size) {
        const end = Math.min(offset + chunkSize, file.size)
        const alreadyReceived = status.received_ranges.some(
            ([rangeStart, rangeEnd]) => rangeStart <= offset && rangeEnd >= end,
        )
        if (!alreadyReceived) {
            queue.push({ start: offset, end, blob: file.slice(offset, end) })
        }
        offset = end
    }

    let completedChunks = totalChunks - queue.length
    updateProgress(completedChunks, fileSize, currentIndex, count)

    async function worker() {
        while (queue.length && !hasError) {
            const { start, end, blob } = queue.shift()!
            const form = new FormData()
            form.append('fileName', file.name)
            form.append('start', start.toString())
            form.append('end', end.toString())
            form.append('size', fileSize.toString())
            form.append('chunk', blob)
            form.append('batch_id', batch_id)

            const resp = await fetch('/api/upload', {
                method: 'POST',
                headers: auth.authHeader,
                body: form,
            })

            if (!resp.ok) {
                hasError = true
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

async function runJob() {
    if (uploading.value) return

    const length = input.value?.files?.length
    if (!length) return

    uploading.value = true
    store.progress = 0
    error.value = ''
    store.progressShow = true

    let hasError = false

    try {
        for (const [i, file] of Array.from(input.value.files as FileList).entries()) {
            const currentFile = file as File
            const fileKey = `${currentFile.name}:${currentFile.size}:${currentFile.lastModified}`
            if (completedFiles.has(fileKey)) {
                store.progress = Math.round(((i + 1) / length) * 100)
                continue
            }

            try {
                await uploadFile(currentFile, batchId.value, i, length)
                completedFiles.add(fileKey)
            } catch (err: any) {
                error.value = err.message || t('upload.failed')
                store.msgAlert('error', error.value)
                hasError = true
                break
            }
        }

        if (!hasError) {
            store.progress = 100
            store.msgAlert('success', t('upload.complete'))
            batchId.value = shortID()
            completedFiles.clear()
        }
    } finally {
        uploading.value = false
        setTimeout(() => {
            store.progressShow = false
            store.progress = 0
        })
    }
}

async function onFileChange(e: Event) {
    input.value = e.target as HTMLInputElement
    batchId.value = shortID()
    completedFiles.clear()
    error.value = ''
}
</script>

<template>
    <div>
        <fieldset class="fieldset">
            <legend class="fieldset-legend">{{ $t('upload.pickFiles') }}</legend>
            <input type="file" class="file-input w-full" :disabled="uploading" @change="onFileChange" multiple />
        </fieldset>

        <p v-if="error" style="color: red">{{ error }}</p>
    </div>
</template>
