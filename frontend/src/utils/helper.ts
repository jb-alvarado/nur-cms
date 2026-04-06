export function closeDropdown(event: Event) {
    const target = event.target as HTMLElement

    setTimeout(() => {
        ;(target.parentNode as HTMLElement | null)?.removeAttribute('open')
    }, 170)
}

export function formatBytes(bytes: number | undefined, dp = 2): string {
    if (!bytes) {
        return '0.0B'
    }

    const thresh = 1024

    if (Math.abs(bytes) < thresh) {
        return bytes + ' B'
    }

    const units = ['KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB', 'ZiB', 'YiB']
    let u = -1
    const r = 10 ** dp

    do {
        bytes /= thresh
        ++u
    } while (Math.round(Math.abs(bytes) * r) / r >= thresh && u < units.length - 1)

    return bytes.toFixed(dp) + ' ' + units[u]
}

export function shortID(): string {
    const input = 'useandom26T198340PX75pxJACKVERYMINDBUSHWOLFGQZbfghjklqvwyzrict'
    let id = ''

    for (let i = 0; i < 7; i++) {
        id += input[(Math.random() * 64) | 0]
    }

    return id
}

export function mediaPath(media: Media): string {
    if (media.variants && media.variants.length > 0) {
        const variance320 = media.variants.find((v) => v.width === 320)
        if (variance320) {
            return `${media.path}/${variance320.filename}`
        }
    }
    return `${media.path}/${media.filename}`
}

export function iconFrom(type: string | null | undefined): string {
    const t = (type || '').toLowerCase()
    switch (true) {
        case t.includes('zip'):
            return 'bi-file-earmark-zip'
        case t.includes('pdf'):
            return 'bi-file-earmark-pdf'
        case t.includes('audio'):
            return 'bi-file-earmark-music'
        case t.includes('video'):
            return 'bi-file-earmark-play'
        case t.includes('text'):
            return 'bi-file-earmark-text'
        case t.includes('powerpoint'):
            return 'bi-file-earmark-ppt'
        case t.includes('word'):
            return 'bi-file-earmark-word'
        default:
            return 'bi-file-earmark'
    }
}
