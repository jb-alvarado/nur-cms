import { computed, onMounted, ref, watch, type CSSProperties, type Ref } from 'vue'
import { useEventListener, useMediaQuery, useResizeObserver } from '@vueuse/core'

const TOP_GAP = 12

export function useFloatingToolbar(
    anchor: Ref<HTMLElement | null>,
    toolbar: Ref<HTMLElement | null>,
    enabled: () => boolean,
    boundary: () => HTMLElement | null,
) {
    const desktop = useMediaQuery('(min-width: 768px)')
    const floatingStyle = ref<CSSProperties>()
    const toolbarHeight = ref(0)
    const placeholderStyle = computed<CSSProperties | undefined>(() =>
        floatingStyle.value ? { height: `${toolbarHeight.value}px` } : undefined,
    )

    function update() {
        const origin = anchor.value
        const controls = toolbar.value
        const container = boundary()
        if (!enabled() || !desktop.value || !origin || !controls || !container) {
            floatingStyle.value = undefined
            return
        }

        const rect = origin.getBoundingClientRect()
        const bounds = container.getBoundingClientRect()
        const bottom = Math.max(bounds.bottom, bounds.top + container.scrollHeight)
        toolbarHeight.value = controls.getBoundingClientRect().height

        if (rect.top >= TOP_GAP || bottom <= TOP_GAP + toolbarHeight.value || rect.width === 0) {
            floatingStyle.value = undefined
            return
        }

        floatingStyle.value = {
            position: 'fixed',
            top: `${TOP_GAP}px`,
            left: `${rect.left}px`,
            width: `${rect.width}px`,
            zIndex: 20,
        }
    }

    // Capture also observes scrolling inside the admin's main element.
    useEventListener(window, 'scroll', update, { capture: true, passive: true })
    useEventListener(window, 'resize', update, { passive: true })
    useResizeObserver([anchor, toolbar, computed(boundary)], update)
    watch([desktop, enabled, boundary], update, { flush: 'post' })
    onMounted(update)

    return { floatingStyle, placeholderStyle }
}
