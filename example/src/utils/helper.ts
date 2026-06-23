import type {
    ContentEntrySerializer,
    ContentNodeSerializer,
    MediaSerializer,
    NodeSerializer,
} from '../../../frontend/src/types/serialized'

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

export function flattenEntryNodes(entry?: ContentEntrySerializer | null): ContentNodeSerializer[] {
    if (!entry?.nodes) return []

    return entry.nodes.flatMap((node: NodeSerializer) => {
        if (node && typeof node === 'object' && 'blocks' in node && Array.isArray(node.blocks)) {
            return node.blocks
        }

        return [node as ContentNodeSerializer]
    })
}

export function extractAstText(content: unknown): string {
    if (Array.isArray(content)) {
        return content.map(extractAstText).filter(Boolean).join(' ')
    }

    if (!content || typeof content !== 'object') return ''

    const node = content as AstLikeNode
    const selfText = node.text ?? node.alt ?? ''
    const childText = node.children?.map(extractAstText).filter(Boolean).join(' ') ?? ''

    return [selfText, childText].filter(Boolean).join(' ')
}

export function stripHtml(value: string): string {
    return value
        .replace(/<[^>]*>/g, ' ')
        .replace(/\s+/g, ' ')
        .trim()
}

export function entryExcerpt(entry: ContentEntrySerializer, maxLength = 180): string {
    const text = flattenEntryNodes(entry)
        .map((node) => node.text ?? node.html ?? extractAstText(node.ast))
        .map((value) => stripHtml(value))
        .filter(Boolean)
        .join(' ')
        .replace(/\s+/g, ' ')
        .trim()

    if (text.length <= maxLength) return text
    return `${text.slice(0, maxLength).replace(/\s+\S*$/, '')}…`
}

export function formatDate(value?: string | null): string {
    if (!value) return ''

    return new Intl.DateTimeFormat(undefined, {
        year: 'numeric',
        month: 'short',
        day: '2-digit',
    }).format(new Date(value))
}
