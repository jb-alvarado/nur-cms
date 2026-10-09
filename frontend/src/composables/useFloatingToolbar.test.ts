import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref, shallowRef, type EffectScope } from 'vue'

import { useFloatingToolbar } from './useFloatingToolbar'

const listeners = vi.hoisted(() => new Map<string, () => void>())

vi.mock('vue', async (original) => ({
    ...(await original<typeof import('vue')>()),
    onMounted: (callback: () => void) => callback(),
}))

vi.mock('@vueuse/core', () => ({
    useMediaQuery: () => desktop,
    useEventListener: (_target: unknown, event: string, callback: () => void) => {
        listeners.set(event, callback)
    },
    useResizeObserver: (_targets: unknown, callback: () => void) => {
        listeners.set('resize-observer', callback)
    },
}))

const desktop = ref(true)
let scope: EffectScope

function rectangle(top: number, height: number, left = 240, width = 800): DOMRect {
    return {
        top,
        bottom: top + height,
        left,
        right: left + width,
        width,
        height,
        x: left,
        y: top,
        toJSON: () => ({}),
    }
}

function setup() {
    const enabled = ref(true)
    const anchorRect = ref(rectangle(100, 48))
    const boundaryRect = ref(rectangle(0, 900))
    const scrollHeight = ref(900)
    const anchor = shallowRef({ getBoundingClientRect: () => anchorRect.value } as HTMLElement)
    const toolbar = shallowRef({ getBoundingClientRect: () => rectangle(0, 48) } as HTMLElement)
    const boundary = {
        getBoundingClientRect: () => boundaryRect.value,
        get scrollHeight() {
            return scrollHeight.value
        },
    } as HTMLElement
    const state = scope.run(() => useFloatingToolbar(anchor, toolbar, () => enabled.value, () => boundary))!

    return { ...state, enabled, anchorRect, boundaryRect, scrollHeight }
}

beforeEach(() => {
    vi.stubGlobal('window', {})
    desktop.value = true
    listeners.clear()
    scope = effectScope()
})

afterEach(() => {
    scope.stop()
    vi.unstubAllGlobals()
})

describe('floating editor toolbar', () => {
    it('floats at the viewport edge and preserves layout, then restores on scrolling back', () => {
        const state = setup()
        expect(state.floatingStyle.value).toBeUndefined()

        state.anchorRect.value = rectangle(-100, 48)
        listeners.get('scroll')?.()
        expect(state.floatingStyle.value).toMatchObject({ position: 'fixed', top: '12px', width: '800px' })
        expect(state.placeholderStyle.value).toEqual({ height: '48px' })

        state.anchorRect.value = rectangle(100, 48)
        listeners.get('scroll')?.()
        expect(state.floatingStyle.value).toBeUndefined()
        expect(state.placeholderStyle.value).toBeUndefined()
    })

    it('stays visible while a longer preview extends beyond the editor height', () => {
        const state = setup()
        state.anchorRect.value = rectangle(-600, 48)
        state.boundaryRect.value = rectangle(-900, 900)
        state.scrollHeight.value = 2000
        listeners.get('scroll')?.()
        expect(state.floatingStyle.value?.position).toBe('fixed')

        state.boundaryRect.value = rectangle(-2000, 900)
        listeners.get('scroll')?.()
        expect(state.floatingStyle.value).toBeUndefined()
    })

    it('tracks horizontal position and width after resizing', () => {
        const state = setup()
        state.anchorRect.value = rectangle(-100, 48, 300, 650)
        listeners.get('resize')?.()
        expect(state.floatingStyle.value).toMatchObject({ left: '300px', width: '650px' })
    })

    it('restores normal layout when switching to mobile or disabling the feature', async () => {
        const state = setup()
        state.anchorRect.value = rectangle(-100, 48)
        listeners.get('scroll')?.()
        desktop.value = false
        await nextTick()
        expect(state.floatingStyle.value).toBeUndefined()

        desktop.value = true
        await nextTick()
        expect(state.floatingStyle.value?.position).toBe('fixed')
        state.enabled.value = false
        await nextTick()
        expect(state.floatingStyle.value).toBeUndefined()
    })
})
