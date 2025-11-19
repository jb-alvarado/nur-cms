export function closeDropdown(event: Event) {
    const target = event.target as HTMLElement

    setTimeout(() => {
        ;(target.parentNode as HTMLElement | null)?.removeAttribute('open')
    }, 170)
}

export function formatBytes(bytes: number): string {
    if (bytes > 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(2) + ' MB'
    if (bytes > 1024) return (bytes / 1024).toFixed(2) + ' KB'
    return bytes.toFixed(0) + ' B'
}

export function shortID(): string {
    const input = 'useandom26T198340PX75pxJACKVERYMINDBUSHWOLFGQZbfghjklqvwyzrict'
    let id = ''

    for (let i = 0; i < 7; i++) {
        id += input[(Math.random() * 64) | 0]
    }

    return id
}
