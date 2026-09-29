import type { MediaProgress } from '@/types/sse'

export function completeMediaProgress(progress: Record<number, MediaProgress>, mediaId: number) {
    progress[mediaId] = { phase: 'completed', percent: 100 }
    const completed = progress[mediaId]

    setTimeout(() => {
        if (progress[mediaId] === completed) delete progress[mediaId]
    }, 3000)
}

export function resetMediaProgress(progress: Record<number, MediaProgress>) {
    for (const mediaId of Object.keys(progress)) delete progress[Number(mediaId)]
}
