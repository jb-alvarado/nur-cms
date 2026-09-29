import Hls from 'hls.js'

export function attachHls(video: HTMLVideoElement, url: string): () => void {
    if (video.canPlayType('application/vnd.apple.mpegurl')) {
        video.src = url
        return () => {
            video.pause()
            video.removeAttribute('src')
            video.load()
        }
    }

    if (!Hls.isSupported()) return () => {}

    const player = new Hls()
    player.loadSource(url)
    player.attachMedia(video)
    return () => {
        video.pause()
        player.destroy()
    }
}
