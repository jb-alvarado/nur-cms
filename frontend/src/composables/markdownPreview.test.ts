import { describe, expect, it } from 'vitest'

import { splitMarkdownPreviewBatches, type MarkdownPreviewInput } from './markdownPreview'

function nodes(count: number, markdown = 'Text'): MarkdownPreviewInput[] {
    return Array.from({ length: count }, (_, index) => ({ key: String(index), markdown }))
}

describe('splitMarkdownPreviewBatches', () => {
    it('keeps batches within the backend node limit', () => {
        expect(splitMarkdownPreviewBatches(nodes(129)).map((batch) => batch.length)).toEqual([128, 1])
    })

    it('counts UTF-8 bytes when applying the Markdown size limit', () => {
        const batches = splitMarkdownPreviewBatches(nodes(2, 'ä'.repeat(256 * 1024)))

        expect(batches.map((batch) => batch.length)).toEqual([1, 1])
    })
})
