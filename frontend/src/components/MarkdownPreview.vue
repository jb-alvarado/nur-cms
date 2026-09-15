<script setup lang="ts">
import { useElementVisibility } from '@vueuse/core'
import { onBeforeUnmount, reactive, ref, watch } from 'vue'
import { mediaPath } from '@/utils/helper'
import { renderMarkdownPreview, type MarkdownPreviewInput } from '@/composables/markdownPreview'

const props = defineProps({
    nodes: {
        type: Array as () => NodeSerializer[],
        default: () => [],
    },
})

const previewRoot = ref<HTMLElement | null>(null)
const previewIsVisible = useElementVisibility(previewRoot)
const htmlByIndex = reactive(new Map<number, string>())
const renderedSignatures = new Map<number, string>()
let debounceTimer: ReturnType<typeof setTimeout> | undefined
let previewController: AbortController | undefined

function isTextNode(node: NodeSerializer): node is ContentNodeSerializer {
    return !('blocks' in node) && 'text' in node
}

async function updatePreview() {
    if (!previewIsVisible.value) return

    const changed: MarkdownPreviewInput[] = []
    const requestedSignatures = new Map<number, string>()
    const activeIndexes = new Set<number>()

    props.nodes.forEach((node, index) => {
        if (!isTextNode(node)) return

        activeIndexes.add(index)
        const markdown = node.text ?? ''
        const signature = JSON.stringify([markdown, node.embeds ?? []])
        if (renderedSignatures.get(index) === signature) return

        changed.push({ key: String(index), markdown, media: node.embeds ?? [] })
        requestedSignatures.set(index, signature)
    })

    for (const index of htmlByIndex.keys()) {
        if (!activeIndexes.has(index)) {
            htmlByIndex.delete(index)
            renderedSignatures.delete(index)
        }
    }

    if (changed.length === 0) return

    previewController?.abort()
    const controller = new AbortController()
    previewController = controller

    try {
        const response = await renderMarkdownPreview(changed, controller.signal)
        if (controller.signal.aborted) return

        for (const node of response.nodes) {
            const index = Number.parseInt(node.key, 10)
            const signature = requestedSignatures.get(index)
            if (!Number.isInteger(index) || signature === undefined) continue

            htmlByIndex.set(index, node.html)
            renderedSignatures.set(index, signature)
        }
    } catch (error) {
        if (!(error instanceof DOMException && error.name === 'AbortError')) {
            console.error('Markdown preview failed', error)
        }
    }
}

function schedulePreview() {
    if (debounceTimer !== undefined) clearTimeout(debounceTimer)
    previewController?.abort()
    if (!previewIsVisible.value) return

    debounceTimer = setTimeout(updatePreview, 400)
}

watch([() => props.nodes, previewIsVisible], schedulePreview, { deep: true, immediate: true })

onBeforeUnmount(() => {
    if (debounceTimer !== undefined) clearTimeout(debounceTimer)
    previewController?.abort()
})
</script>
<template>
    <div ref="previewRoot" class="overflow-auto h-full">
        <template v-for="(node, i) in nodes" :key="i">
            <div v-if="'blocks' in node" class="rounded flex flex-col gap-2 mt-2 border border-base-content/30">
                <div
                    v-for="(block, bi) in node.blocks"
                    :key="block.id ?? bi"
                    class="flex bg-base-200 rounded p-2 gap-1"
                >
                    <div class="w-10">
                        <img
                            v-if="block.media"
                            :src="mediaPath(block.media!)"
                            :atl="block.media?.alt"
                            class="object-cover w-10 h-10"
                        />
                        <div v-else class="bg-base-content/30 w-full h-10"></div>
                    </div>
                    <div class="flex flex-col gap-2 grow">
                        <div
                            v-for="(_, key) in (block.data as Record<string, any>) ?? {}"
                            :key="key"
                            class="flex items-center gap-2 grow"
                        >
                            <label class="min-w-20">{{ key }}: </label>
                            <input
                                v-model="(block.data as Record<string, any>)[key]"
                                type="text"
                                class="input grow border border-base-content/10"
                                disabled
                            />
                        </div>
                    </div>
                </div>
            </div>
            <div
                v-else-if="'text' in node"
                v-html="htmlByIndex.get(i) ?? ''"
                class="prose max-w-full overflow-auto bg-base-200 p-4 rounded border border-base-content/25"
                :class="{ 'mt-2': i > 0 }"
            />
            <div
                v-else-if="'data' in node && node.data && typeof node.data === 'object' && !Array.isArray(node.data)"
                class="flex bg-base-200 p-2 gap-1"
                :class="{ 'mt-2': i > 0 }"
            >
                <div class="w-10">
                    <img
                        v-if="node.media"
                        :src="mediaPath(node.media!)"
                        :atl="node.media?.alt"
                        class="object-cover w-10 h-10"
                    />
                    <div v-else class="bg-base-content/30 w-full h-10"></div>
                </div>
                <div class="flex flex-col gap-2 grow">
                    <div v-for="(_, key) in node.data" :key="key" class="flex items-center gap-2 grow">
                        <label class="min-w-20">{{ key }}: </label>
                        <input
                            v-model="node.data[key]"
                            type="text"
                            class="input grow border border-base-content/10"
                            disabled
                        />
                    </div>
                </div>
            </div>
        </template>
    </div>
</template>
