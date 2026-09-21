import { describe, expect, it } from 'vitest'
import { faviconLink } from './favicon'

describe('faviconLink', () => {
    it.each([
        'image/avif',
        'image/gif',
        'image/jpeg',
        'image/png',
        'image/svg+xml',
        'image/vnd.microsoft.icon',
        'image/webp',
        'image/x-icon',
    ])(
        'uses a logo with the supported MIME type %s',
        (mimeType) => {
            expect(faviconLink('/uploads/logo?v=42', mimeType)).toEqual({
                href: '/uploads/logo?v=42',
                type: mimeType,
            })
        },
    )

    it('normalizes the non-standard image/jpg MIME type', () => {
        expect(faviconLink('/uploads/logo.jpg?v=42', 'image/jpg')).toEqual({
            href: '/uploads/logo.jpg?v=42',
            type: 'image/jpeg',
        })
    })

    it.each([
        [null, 'image/png'],
        ['/uploads/logo.heic?v=42', 'image/heic'],
        ['/uploads/logo.png?v=42', null],
    ])('keeps the default favicon for an unsupported logo', (url, mimeType) => {
        expect(faviconLink(url, mimeType)).toEqual({
            href: `${import.meta.env.BASE_URL}favicon.ico`,
            type: 'image/x-icon',
        })
    })
})
