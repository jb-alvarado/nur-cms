<script setup lang="ts">
import { marked } from 'marked'
import { mediaPath } from '@/utils/helper'

const renderer = {
    image(token: any) {
        const title = token.title ? ` title="${token.title}"` : ''
        const alt = token.text ? ` alt="${token.text}"` : ''
        return `<img src="${token.href}"${alt}${title} class="w-40 not-prose">`
    },

    paragraph(token: any) {
        const t = token.tokens

        if (t.length === 1 && t[0].type === 'image') {
            const token = t[0]
            const title = token.title ? ` title="${token.title}"` : ''
            const alt = token.text ? ` alt="${token.text}"` : ''
            return `<div class="mx-auto">
                        <img src="${token.href}"${alt}${title}>
                    </div>`
        }

        const innerTokens: any[] = []
        let html = ''

        t.forEach((tok: any, i: number) => {
            if (tok.type === 'image') {
                const floatClass = i === 0 ? 'float-left mr-4 mb-2' : 'float-right ml-4 mb-2'
                const title = tok.title ? ` title="${tok.title}"` : ''
                const alt = tok.text ? ` alt="${tok.text}"` : ''
                html += `<img src="${tok.href}"${alt}${title} class="w-60 not-prose ${floatClass}">`
            } else {
                innerTokens.push(tok)
            }
        })

        const inner = (this as any).parser.parseInline(innerTokens)
        return `${html}<p>${inner}</p>`
    },
}

marked.use({ renderer })

defineProps({
    nodes: {
        type: Array as () => NodeSerializer[],
        default: () => [],
    },
})
</script>
<template>
    <div class="overflow-auto h-full">
        <template v-for="(node, i) in nodes" :key="i">
            <div v-if="'blocks' in node" class="rounded flex flex-col gap-2 mt-2 border border-base-content/30">
                <div
                    v-for="(block, bi) in node.blocks"
                    :key="block.id ?? bi"
                    class="bg-base-200 rounded p-2 flex gap-1"
                >
                    <div class="w-10">
                        <img
                            v-if="block.media"
                            :src="mediaPath(block.media!)"
                            :atl="block.media?.alt"
                            class="object-cover w-10 h-10"
                        />
                        <div v-else class="bg-base-content/30 w-full h-10"></div>
                    </div>
                    <div class="flex flex-col gap-2 grow">
                        <div
                            v-for="(_, key) in (block.data as Record<string, any>) ?? {}"
                            :key="key"
                            class="flex items-center gap-2 grow"
                        >
                            <label class="min-w-20">{{ key }}: </label>
                            <input
                                v-model="(block.data as Record<string, any>)[key]"
                                type="text"
                                class="input grow border border-base-content/10"
                                disabled
                            />
                        </div>
                    </div>
                </div>
            </div>
            <div
                v-else-if="'text' in node"
                v-html="marked(node.text ?? '')"
                class="prose max-w-full overflow-auto bg-base-200 p-4 rounded border border-base-content/25"
            />
            <div
                v-else-if="'data' in node && node.data && typeof node.data === 'object' && !Array.isArray(node.data)"
                class="flex items-center gap-1 bg-base-200 p-2"
            >
                <div class="w-10">
                    <img
                        v-if="node.media"
                        :src="mediaPath(node.media!)"
                        :atl="node.media?.alt"
                        class="object-cover w-10 h-10"
                    />
                    <div v-else class="bg-base-content/30 w-full h-10"></div>
                </div>
                <div v-for="(_, key) in node.data" :key="key" class="flex items-center gap-2 grow">
                    <label class="min-w-20">{{ key }}: </label>
                    <input v-model="node.data[key]" type="text" class="input grow border border-base-content/10" disabled />
                </div>
            </div>
        </template>
    </div>
</template>
