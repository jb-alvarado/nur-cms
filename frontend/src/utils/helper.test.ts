import { describe, expect, it } from 'vitest'
import { mediaThumbnailPath, randomID, shortID } from './helper'

describe('mediaThumbnailPath', () => {
    it('uses the smallest video poster instead of a video file', () => {
        const media: Media = {
            type: 'video/mp4',
            path: '/uploads/2026/09',
            filename: 'clip.mp4',
            variants: [
                { id: 1, width: 1280, height: 720, filename: 'clip--poster-1280.webp' },
                { id: 2, width: 320, height: 180, filename: 'clip--poster-320.webp' },
            ],
        }

        expect(mediaThumbnailPath(media)).toBe('/uploads/2026/09/clip--poster-320.webp')
    })

    it('keeps image thumbnails and falls back when a video has no poster', () => {
        expect(
            mediaThumbnailPath({
                type: 'image/jpeg',
                path: '/uploads/2026/09',
                filename: 'photo.jpg',
                variants: [{ id: 3, width: 320, height: 180, filename: 'photo-320.webp' }],
            }),
        ).toBe('/uploads/2026/09/photo-320.webp')
        expect(mediaThumbnailPath({ type: 'video/mp4', path: '/uploads', filename: 'clip.mp4' })).toBeUndefined()
    })
})

describe('shortID', () => {
    it('always returns a compact alphanumeric identifier', () => {
        for (let index = 0; index < 100; index++) {
            expect(shortID()).toMatch(/^[a-f0-9]{7}$/)
        }
    })

    it('creates a full-length upload identifier without undefined fragments', () => {
        expect(randomID()).toMatch(/^[a-f0-9]{32}$/)
    })
})
