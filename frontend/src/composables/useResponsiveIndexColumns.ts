import { watch } from 'vue'
import { useMediaQuery } from '@vueuse/core'

type IndexStore = {
    visibleRows: TableRow[]
    contentSelect: () => Promise<void>
}

/**
 * Hides configured table fields on mobile without changing the persisted
 * desktop column selection.
 */
export function useResponsiveIndexColumns(store: IndexStore, mobileHiddenFields: string[]) {
    const isMobile = useMediaQuery('(max-width: 767px)')
    const hiddenFields = new Set(mobileHiddenFields)
    const desktopVisibleRows = store.visibleRows.map((row) => ({ ...row }))

    watch(
        isMobile,
        (mobile, previousMobile) => {
            store.visibleRows = mobile
                ? desktopVisibleRows.filter((row) => !hiddenFields.has(row.field))
                : desktopVisibleRows.map((row) => ({ ...row }))

            if (previousMobile !== undefined) {
                store.contentSelect()
            }
        },
        { immediate: true },
    )
}
