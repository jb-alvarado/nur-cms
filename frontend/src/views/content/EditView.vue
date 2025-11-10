<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { useRoute } from 'vue-router'
// import MarkdownRender from 'vue-renderer-markdown'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { slugify } from '@/utils/slugify.js'

const route = useRoute()
const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type

const auth = useAuth()
const store = useIndex()
const content = ref({} as Content)
const textareaRef = ref()
const lastBody = ref('')
const lastPos = ref(0)
const format = ref(0)

const headings = [
    { name: 'Text', value: 0 },
    { name: 'Heading 1', value: 1 },
    { name: 'Heading 2', value: 2 },
    { name: 'Heading 3', value: 3 },
    { name: 'Heading 4', value: 4 },
    { name: 'Heading 5', value: 5 },
]

fetch(`/api/content/entries/${typeParam}?id=${route.params.id}&output_type=markdown`, {
    headers: auth.authHeader,
})
    .then(async (resp) => {
        if (resp.status >= 400) {
            const text = await resp.text()
            let msg: string

            try {
                const json = JSON.parse(text)
                msg = json?.error ?? (typeof json === 'string' ? json : JSON.stringify(json))
            } catch {
                msg = text
            }
            throw new Error(msg)
        }

        return resp.json()
    })
    .then((response: RespondObj) => {
        content.value = response.results[0]
    })
    .catch((e) => {
        store.msgAlert('error', e, 6)
    })

function updateSlug() {
    if (content.value.title) {
        content.value.slug = slugify(content.value.title)
    }
}

function undo(e: KeyboardEvent) {
    if (e.ctrlKey && e.key === 'z' && lastBody.value) {
        const textarea = textareaRef.value
        if (!textarea) return

        textareaRef.value.value = lastBody.value

        nextTick(() => {
            textarea.setSelectionRange(lastPos.value, lastPos.value)
            textarea.focus()
        })

        lastBody.value = ''
    }
}

function insertMarkdown(before: string, after = '') {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    let selected = textarea.value.substring(start, end)

    lastBody.value = content.value.body
    lastPos.value = start

    if (before.startsWith('[]')) {
        before = before.replace('[]', `[${selected}]`)
        selected = selected.startsWith('http') ? selected : `https://${selected}`
    }

    if (start === end) {
        textarea.setRangeText(before + after, start, end, 'end')
        const cursorPos = start + before.length
        textarea.setSelectionRange(cursorPos, cursorPos)
    } else {
        textarea.setRangeText(before + selected + after, start, end, 'end')
        const cursorPos = start + before.length + selected.length + after.length
        textarea.setSelectionRange(cursorPos, cursorPos)
    }

    textarea.focus()
}

function bold() {
    insertMarkdown('**', '**')
}

function italic() {
    insertMarkdown('*', '*')
}

function underline() {
    insertMarkdown('<u>', '</u>')
}

function strikethrough() {
    insertMarkdown('~~', '~~')
}

function link() {
    insertMarkdown('[](', ')')
}

function image() {
    insertMarkdown('![](', ')')
}

function textPosition() {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const value = textarea.value

    const lineStart = value.lastIndexOf('\n', start - 1) + 1
    const lineEnd = value.indexOf('\n', end)
    const actualLineEnd = lineEnd === -1 ? value.length : lineEnd
    const line = value.slice(lineStart, actualLineEnd)

    const hashCount = line.match(/^#+/)?.[0].length ?? 0
    format.value = hashCount
}

function heading() {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const value = textarea.value

    const lineStart = value.lastIndexOf('\n', start - 1) + 1
    const lineEnd = value.indexOf('\n', end)
    const actualLineEnd = lineEnd === -1 ? value.length : lineEnd

    const line = value.slice(lineStart, actualLineEnd)
    const stripped = line.replace(/^#+\s*/, '')
    const newLine = format.value === 0 ? stripped : `${'#'.repeat(format.value)} ${stripped.trimStart()}`

    lastBody.value = content.value.body
    lastPos.value = start

    content.value.body = value.slice(0, lineStart) + newLine + value.slice(actualLineEnd)

    nextTick(() => {
        const cursorPos = lineStart + newLine.length
        textarea.setSelectionRange(cursorPos, cursorPos)
        textarea.focus()
    })
}

function quote() {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const value = textarea.value

    const lineStart = value.lastIndexOf('\n', start - 1) + 1
    const lineEnd = value.indexOf('\n', end)
    const actualLineEnd = lineEnd === -1 ? value.length : lineEnd

    const line = value.slice(lineStart, actualLineEnd)
    let newLine

    if (line.startsWith('>')) {
        newLine = line.replace(/^>+\s*/, '')
    } else {
        newLine = `> ${line.trimStart()}`
    }

    lastBody.value = content.value.body
    lastPos.value = start

    content.value.body = value.slice(0, lineStart) + newLine + value.slice(actualLineEnd)

    nextTick(() => {
        const cursorPos = lineStart + newLine.length
        textarea.setSelectionRange(cursorPos, cursorPos)
        textarea.focus()
    })
}
</script>

<template>
    <div class="h-full">
        <div class="flex">
            <h1 class="text-2xl grow">{{ content.title }}</h1>
        </div>

        <div class="h-[calc(100%-75px)]  2xl:w-2/3 bg-base-300 p-4 mt-4 rounded">
            <div class="h-10 mb-6 flex gap-4 items-center">
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">Title</legend>
                    <input
                        v-model="content.title"
                        type="text"
                        class="input"
                        placeholder="Title"
                        @keyup="updateSlug()"
                    />
                </fieldset>

                <fieldset class="fieldset">
                    <legend class="fieldset-legend">Slug</legend>
                    <input v-model="content.slug" type="text" class="input" placeholder="Slug" />
                </fieldset>
            </div>

            <div class="py-1.5 join">
                <select v-model="format" class="select select-sm join-item" @change="heading">
                    <option v-for="head in headings" :key="head.value" :value="head.value">{{ head.name }}</option>
                </select>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="bold">
                    <i class="bi bi-type-bold"></i>
                </button>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="italic">
                    <i class="bi bi-type-italic"></i>
                </button>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="underline">
                    <i class="bi bi-type-underline"></i>
                </button>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="strikethrough">
                    <i class="bi bi-type-strikethrough"></i>
                </button>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="link">
                    <i class="bi bi-link-45deg"></i>
                </button>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="image">
                    <i class="bi bi-image"></i>
                </button>
                <button class="btn join-item border-base-content/25 p-2 h-8 leading-0" @click="quote">
                    <i class="bi bi-quote"></i>
                </button>
            </div>
            <div class="flex gap-8 h-[calc(100%-110px)]">
                <textarea
                    ref="textareaRef"
                    v-model="content.body"
                    class="textarea resize-none w-full"
                    @keyup="undo"
                    @click="textPosition"
                ></textarea>
                <!-- <div class="bg-red-900 w-full h-full">RIGHT</div> -->
                <!-- <MarkdownRender :content="content.body" class="prose hidden xl:block w-1/2 bg-base-200 p-4 rounded" /> -->
            </div>
        </div>
    </div>
</template>
