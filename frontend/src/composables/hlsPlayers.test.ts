import { afterEach, describe, expect, it, vi } from 'vitest'
import { createRenderer, h, nextTick, ref } from 'vue'

import { useHlsPlayers } from './hlsPlayers'

const { attachHls, cleanup } = vi.hoisted(() => {
    const cleanup = vi.fn()
    return {
        cleanup,
        attachHls: vi.fn(() => cleanup),
    }
})

vi.mock('@/utils/hls', () => ({ attachHls }))

// A small renderer host lets Vue run the real update/unmount hooks
// without a browser or an additional DOM dependency.
class HostNode {
    parent: HostNode | null = null
    children: HostNode[] = []
    videos: HTMLVideoElement[] = []

    querySelectorAll(): HTMLVideoElement[] {
        return [...this.videos, ...this.children.flatMap((child) => child.querySelectorAll())]
    }
}

function detach(node: HostNode) {
    if (!node.parent) return
    node.parent.children = node.parent.children.filter((child) => child !== node)
    node.parent = null
}

const renderer = createRenderer<HostNode, HostNode>({
    createElement: () => new HostNode(),
    createText: () => new HostNode(),
    createComment: () => new HostNode(),
    setText: () => {},
    setElementText: () => {},
    parentNode: (node) => node.parent,
    nextSibling: (node) => {
        const siblings = node.parent?.children ?? []
        return siblings[siblings.indexOf(node) + 1] ?? null
    },
    insert(node, parent, anchor) {
        detach(node)
        const index = anchor ? parent.children.indexOf(anchor) : parent.children.length
        parent.children.splice(index, 0, node)
        node.parent = parent
    },
    remove: detach,
    patchProp(node, key, _previous, value) {
        if (key !== 'innerHTML') return
        const html = String(value ?? '')
        node.videos = Array.from(html.matchAll(/data-hls-src="([^"]+)"/g), (match) => (
            { dataset: { hlsSrc: match[1] } } as unknown as HTMLVideoElement
        ))
    },
})

afterEach(() => {
    vi.clearAllMocks()
})

describe('MarkdownPreview HLS lifecycle', () => {
    it('initializes existing videos and reloads a changed source on the same node', async () => {
        const revision = ref(0)
        const app = renderer.createApp({
            setup() {
                const root = ref<HTMLElement | null>(null)
                useHlsPlayers(root)
                return () => h('div', {
                    ref: root,
                    'data-revision': revision.value,
                    innerHTML: '<video data-hls-src="/uploads/clip/master.m3u8"></video>',
                })
            },
        })
        const container = new HostNode()
        app.mount(container)
        try {
            expect(attachHls).toHaveBeenCalledTimes(1)
            const video = container.children[0]!.videos[0]!
            video.dataset.hlsSrc = '/uploads/clip/master.m3u8?subtitles=1'
            revision.value++
            await nextTick()
            expect(cleanup).toHaveBeenCalledTimes(1)
            expect(attachHls).toHaveBeenLastCalledWith(video, '/uploads/clip/master.m3u8?subtitles=1')
            revision.value++
            await nextTick()
            expect(attachHls).toHaveBeenCalledTimes(2)
        } finally {
            app.unmount()
        }
        expect(cleanup).toHaveBeenCalledTimes(2)
    })

    it('releases a player when its text block is removed', async () => {
        const showVideo = ref(false)
        const app = renderer.createApp({
            setup() {
                const root = ref<HTMLElement | null>(null)
                useHlsPlayers(root)
                return () => h('div', { ref: root }, showVideo.value ? [
                    h('div', { innerHTML: '<video data-hls-src="/uploads/clip/master.m3u8"></video>' }),
                ] : [])
            },
        })
        app.mount(new HostNode())

        try {
            showVideo.value = true
            await nextTick()
            expect(attachHls).toHaveBeenCalledTimes(1)

            showVideo.value = false
            await nextTick()

            expect(cleanup).toHaveBeenCalledTimes(1)
        } finally {
            app.unmount()
        }
        expect(cleanup).toHaveBeenCalledTimes(1)
    })
})
