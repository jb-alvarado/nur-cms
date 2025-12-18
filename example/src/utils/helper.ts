import type { MediaSerializer } from '../../../frontend/src/types/serialized'

export function mediaPath(media: MediaSerializer): string {
    if (media.variants && media.variants.length > 0) {
        const variance320 = media.variants.find((v) => v.width === 320)
        if (variance320) {
            return `${media.path}/${variance320.filename}`
        }
    }
    return `${media.path}/${media.filename}`
}
