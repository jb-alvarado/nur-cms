import { describe, expect, it } from 'vitest'
import { filenameFromProcessingMessage } from './mediaProcessing'

describe('media processing events', () => {
    it.each([
        ['Video thumbnail done: clip.mp4', 'clip.mp4'],
        ['Video thumbnail failed: clip.mp4', 'clip.mp4'],
        ['Video processing failed: other.webm', 'other.webm'],
        ['Variants done: image.jpg', 'image.jpg'],
    ])('extracts the affected filename from %s', (message, filename) => {
        expect(filenameFromProcessingMessage(message)).toBe(filename)
    })

    it('ignores unrelated and incomplete messages', () => {
        expect(filenameFromProcessingMessage('Upload done: clip.mp4')).toBeUndefined()
        expect(filenameFromProcessingMessage('Video thumbnail done: ')).toBeUndefined()
    })
})
