import type { MediaSerializer } from '../../../frontend/src/types/serialized'

interface AstLikeNode {
    type?: string
    text?: string
    alt?: string
    path?: string
    filename?: string
    src?: string
    variants?: MediaSerializer['variants']
    children?: AstLikeNode[]
}

function joinPath(path?: string | null, filename?: string | null): string {
    if (!path || !filename) return ''
    return `${path.replace(/\/$/, '')}/${filename}`
}

export function mediaPath(media?: MediaSerializer | null, preferredWidth = 640): string {
    if (!media) return ''

    const variants = media.variants ?? []
    const variant =
        variants.find((v) => v.width === preferredWidth) ??
        variants.find((v) => v.width === 320) ??
        [...variants].sort((a, b) => a.width - b.width)[0]

    if (variant) {
        return joinPath(media.path, variant.filename)
    }

    return joinPath(media.path, media.filename)
}

export function astMediaPath(node: AstLikeNode, preferredWidth = 640): string {
    if (node.src) return node.src

    return mediaPath(
        {
            path: node.path,
            filename: node.filename,
            variants: node.variants ?? [],
        },
        preferredWidth,
    )
}

export function extractAstText(content: unknown): string {
    if (Array.isArray(content)) {
        return content.map(extractAstText).filter(Boolean).join(' ')
    }

    if (!content || typeof content !== 'object') return ''

    const node = content as AstLikeNode
    const selfText = node.type === 'html' ? '' : (node.text ?? node.alt ?? '')
    const childText = node.children?.map(extractAstText).filter(Boolean).join(' ') ?? ''

    return [selfText, childText].filter(Boolean).join(' ')
}

export function formatDate(value?: string | null): string {
    if (!value) return ''

    return new Intl.DateTimeFormat(undefined, {
        year: 'numeric',
        month: 'short',
        day: '2-digit',
    }).format(new Date(value))
}
