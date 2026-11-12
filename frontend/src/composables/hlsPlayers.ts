import { onBeforeUnmount, onMounted, onUpdated, type Ref } from 'vue'

import { attachHls } from '@/utils/hls'

export function useHlsPlayers(root: Ref<HTMLElement | null>) {
    const players = new Map<HTMLVideoElement, { url: string; cleanup: () => void }>()

    function syncPlayers() {
        const videos = root.value?.querySelectorAll<HTMLVideoElement>('video[data-hls-src]') ?? []
        const active = new Set(videos)
        for (const [video, player] of players) {
            if (active.has(video) && video.dataset.hlsSrc === player.url) continue
            player.cleanup()
            players.delete(video)
        }
        for (const video of videos) {
            if (players.has(video)) continue
            const url = video.dataset.hlsSrc
            if (url) players.set(video, { url, cleanup: attachHls(video, url) })
        }
    }

    onMounted(syncPlayers)
    onUpdated(syncPlayers)

    onBeforeUnmount(() => {
        for (const player of players.values()) player.cleanup()
        players.clear()
    })
}
