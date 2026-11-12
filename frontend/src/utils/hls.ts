import '@/utils/videoPlayer'

export function attachHls(video: HTMLVideoElement, url: string): () => void {
    const provider = document.createElement('media-i18n')
    const player = document.createElement('video-player')
    const skin = document.createElement('video-skin')
    const media = document.createElement('hls-video')
    const wasHidden = video.hidden

    provider.style.display = 'block'
    player.className = video.className
    const poster = video.getAttribute('poster')
    if (poster) player.setAttribute('poster', poster)
    skin.style.display = 'block'
    skin.style.width = '100%'
    skin.style.aspectRatio = '16 / 9'
    for (const attribute of ['poster', 'preload', 'crossorigin', 'autoplay', 'loop', 'muted', 'title']) {
        const value = video.getAttribute(attribute)
        if (value !== null) media.setAttribute(attribute, value)
    }
    media.setAttribute('playsinline', '')
    media.src = url
    for (const track of video.querySelectorAll('track')) media.append(track.cloneNode(true))
    skin.append(media)
    player.append(skin)
    provider.append(player)

    // Keep Vue's original node in place so updates and unmounting retain ownership.
    video.pause()
    video.hidden = true
    video.after(provider)

    return () => {
        media.pause()
        media.removeAttribute('src')
        provider.remove() // Disconnecting the media element destroys its playback engine.
        video.hidden = wasHidden
    }
}
