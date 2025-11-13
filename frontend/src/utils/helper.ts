export function closeDropdown(event: Event) {
    const target = event.target as HTMLElement

    setTimeout(() => {
        ;(target.parentNode as HTMLElement | null)?.removeAttribute('open')
    }, 170)
}
