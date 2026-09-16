export interface FaviconLink {
    href: string
    type: string
}

const DEFAULT_FAVICON: FaviconLink = {
    href: '/favicon.ico',
    type: 'image/x-icon',
}

const FAVICON_MIME_TYPES = new Set([
    'image/avif',
    'image/gif',
    'image/jpeg',
    'image/jpg',
    'image/png',
    'image/svg+xml',
    'image/vnd.microsoft.icon',
    'image/webp',
    'image/x-icon',
])

export function faviconLink(logoUrl: string | null, logoMimeType: string | null): FaviconLink {
    if (!logoUrl || !logoMimeType || !FAVICON_MIME_TYPES.has(logoMimeType)) {
        return DEFAULT_FAVICON
    }

    return {
        href: logoUrl,
        type: logoMimeType === 'image/jpg' ? 'image/jpeg' : logoMimeType,
    }
}
