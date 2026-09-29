import { reactive } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MediaProgress } from '@/types/sse'
import { completeMediaProgress, resetMediaProgress } from './mediaProgress'

describe('media progress lifecycle', () => {
    afterEach(() => vi.useRealTimers())

    it('removes completed progress from reactive state after three seconds', () => {
        vi.useFakeTimers()
        const progress = reactive<Record<number, MediaProgress>>({})
        completeMediaProgress(progress, 42)
        expect(progress[42]).toEqual({ phase: 'completed', percent: 100 })

        vi.advanceTimersByTime(3000)
        expect(progress[42]).toBeUndefined()
    })

    it('does not remove progress belonging to a new processing run', () => {
        vi.useFakeTimers()
        const progress = reactive<Record<number, MediaProgress>>({})
        completeMediaProgress(progress, 42)
        progress[42] = { phase: 'encoding', percent: 5 }

        vi.advanceTimersByTime(3000)
        expect(progress[42]).toEqual({ phase: 'encoding', percent: 5 })
    })

    it('discards stale state on disconnect and accepts the next heartbeat', () => {
        const progress = reactive<Record<number, MediaProgress>>({
            42: { phase: 'encoding', percent: 64 },
            43: { phase: 'imageVariants', percent: 45 },
        })
        resetMediaProgress(progress)
        expect(Object.keys(progress)).toHaveLength(0)

        progress[43] = { phase: 'imageVariants', percent: 90 }
        expect(progress[42]).toBeUndefined()
        expect(progress[43].percent).toBe(90)
    })
})
