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
