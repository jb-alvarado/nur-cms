import { onScopeDispose, watch } from 'vue'
import { useMediaQuery } from '@vueuse/core'

type IndexStore = {
    responsiveHiddenFields: string[]
}

/**
 * Hides configured table fields on mobile without changing the persisted
 * desktop column selection.
 */
export function useResponsiveIndexColumns(store: IndexStore, mobileHiddenFields: string[]) {
    const isMobile = useMediaQuery('(max-width: 767px)')

    watch(
        isMobile,
        (mobile) => {
            store.responsiveHiddenFields = mobile ? [...mobileHiddenFields] : []
        },
        { immediate: true },
    )

    onScopeDispose(() => {
        store.responsiveHiddenFields = []
    })
}
