import { authFetch } from '@/composables/authFetch'
import type { MediaSerializer } from '@/types/serialized.d'

export type MarkdownPreviewInput = {
    key: string
    markdown: string
    media?: MediaSerializer[]
}

type MarkdownPreviewResponse = {
    nodes: Array<{
        key: string
        html: string
    }>
}

const MAX_PREVIEW_NODES_PER_REQUEST = 128
const MAX_PREVIEW_MARKDOWN_BYTES_PER_REQUEST = 512 * 1024

export function splitMarkdownPreviewBatches(nodes: MarkdownPreviewInput[]): MarkdownPreviewInput[][] {
    const batches: MarkdownPreviewInput[][] = []
    let batch: MarkdownPreviewInput[] = []
    let markdownBytes = 0

    for (const node of nodes) {
        const nodeBytes = new TextEncoder().encode(node.markdown).byteLength
        if (
            batch.length > 0 &&
            (batch.length >= MAX_PREVIEW_NODES_PER_REQUEST ||
                markdownBytes + nodeBytes > MAX_PREVIEW_MARKDOWN_BYTES_PER_REQUEST)
        ) {
            batches.push(batch)
            batch = []
            markdownBytes = 0
        }

        batch.push(node)
        markdownBytes += nodeBytes
    }

    if (batch.length > 0) batches.push(batch)
    return batches
}

async function renderMarkdownPreviewBatch(nodes: MarkdownPreviewInput[], signal?: AbortSignal) {
    return authFetch<MarkdownPreviewResponse>('/api/markdown/preview', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ nodes }),
        signal,
    })
}

export async function renderMarkdownPreview(
    nodes: MarkdownPreviewInput[],
    signal?: AbortSignal,
): Promise<MarkdownPreviewResponse> {
    const rendered: MarkdownPreviewResponse['nodes'] = []

    for (const batch of splitMarkdownPreviewBatches(nodes)) {
        const response = await renderMarkdownPreviewBatch(batch, signal)
        rendered.push(...response.nodes)
    }

    return { nodes: rendered }
}
