import { onBeforeUnmount, onUpdated, type Ref } from 'vue'

import { attachHls } from '@/utils/hls'

export function useHlsPlayers(root: Ref<HTMLElement | null>) {
    const players = new Map<HTMLVideoElement, () => void>()

    onUpdated(() => {
        const videos = root.value?.querySelectorAll<HTMLVideoElement>('video[data-hls-src]') ?? []
        const active = new Set(videos)
        for (const [video, cleanup] of players) {
            if (active.has(video)) continue
            cleanup()
            players.delete(video)
        }
        for (const video of videos) {
            if (players.has(video)) continue
            const url = video.dataset.hlsSrc
            if (url) players.set(video, attachHls(video, url))
        }
    })

    onBeforeUnmount(() => {
        for (const cleanup of players.values()) cleanup()
        players.clear()
    })
}
