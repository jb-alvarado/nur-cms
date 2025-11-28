<script setup lang="ts">
import { computed } from 'vue'
import { marked } from 'marked'

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

const props = defineProps({
    text: {
        type: String,
        default: '',
    },
})

const html = computed(() => marked(props.text ?? ''))
</script>
<template>
    <div
        v-html="html"
        class="prose max-w-[800px] h-full overflow-auto bg-base-100 p-4 rounded border border-base-content/25"
    />
</template>
