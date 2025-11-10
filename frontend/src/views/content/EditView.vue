<script setup lang="ts">
import { ref, nextTick, computed } from 'vue'
import { useRoute } from 'vue-router'
import { cloneDeep, isEqual } from 'lodash-es'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { slugify } from '@/utils/slugify.js'

const route = useRoute()
const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type

const auth = useAuth()
const store = useIndex()
const content = ref({} as Content)
const contentOriginal = ref({} as Content)
const textareaRef = ref()
const lastBody = ref('')
const lastPos = ref(0)
const format = ref(0)
const needsSave = computed(() => !isEqual(content.value, contentOriginal.value))

const headings = [
    { name: 'Text', value: 0 },
    { name: 'Heading 1', value: 1 },
    { name: 'Heading 2', value: 2 },
    { name: 'Heading 3', value: 3 },
    { name: 'Heading 4', value: 4 },
    { name: 'Heading 5', value: 5 },
]

const editorButtons = [
    { id: 0, icon: 'bi-type-bold', func: bold },
    { id: 1, icon: 'bi-type-italic', func: italic },
    { id: 2, icon: 'bi-type-underline', func: underline },
    { id: 3, icon: 'bi-type-strikethrough', func: strikethrough },
    { id: 4, icon: 'bi-link-45deg', func: link },
    { id: 5, icon: 'bi-image', func: image },
    { id: 6, icon: 'bi-quote', func: quote },
]

const status = ['draft', 'published', 'archived']

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
        contentOriginal.value = cloneDeep(content.value)
    })
    .catch((e) => {
        store.msgAlert('error', e, 6)
    })

function updateSlug() {
    if (content.value.title) {
        content.value.slug = slugify(content.value.title)
    }
}

function keyHandler(e: KeyboardEvent) {
    const textarea = textareaRef.value
    if (!textarea) return

    if (e.ctrlKey && e.key === 'z' && lastBody.value) {
        e.preventDefault()

        content.value.body = lastBody.value

        nextTick(() => {
            textarea.setSelectionRange(lastPos.value, lastPos.value)
            textarea.focus()
        })

        lastBody.value = ''
    } else if (e.key === 'Tab') {
        const start = textarea.selectionStart
        const end = textarea.selectionEnd
        const value = textarea.value

        const tab = '    '

        lastBody.value = content.value.body
        lastPos.value = start

        if (start === end) {
            content.value.body = value.slice(0, start) + tab + value.slice(end)
            nextTick(() => {
                textarea.setSelectionRange(start + tab.length, start + tab.length)
            })
        } else {
            const before = value.slice(0, start)
            const selection = value.slice(start, end)
            const after = value.slice(end)

            const indented = selection
                .split('\n')
                .map((line: string) => tab + line)
                .join('\n')

            content.value.body = before + indented + after

            nextTick(() => {
                textarea.setSelectionRange(start + tab.length, end + tab.length * selection.split('\n').length)
            })
        }
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

    const value = content.value.body
    let cursorPos = start + before.length

    if (start === end) {
        const newValue = value.slice(0, start) + before + after + value.slice(end)
        content.value.body = newValue
    } else {
        const newValue = value.slice(0, start) + before + selected + after + value.slice(end)
        content.value.body = newValue
        cursorPos = start + before.length + selected.length + after.length
    }

    nextTick(() => {
        textarea.setSelectionRange(cursorPos, cursorPos)
        textarea.focus()
    })
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
    insertMarkdown('![', ']()')
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
    <div class="flex flex-col h-full pb-6">
        <div class="flex-none">
            <h1 class="text-2xl">{{ content.title }}</h1>
        </div>

        <!-- Form + Editor Container -->
        <div class="flex flex-col flex-1 2xl:w-2/3 bg-base-300 p-4 pt-1 mt-4 rounded overflow-hidden">
            <!-- Form inputs -->
            <div class="flex items-center flex-wrap gap-4 flex-none">
                <div class="grow flex flex-col md:flex-row gap-2">
                    <fieldset class="fieldset w-80">
                        <legend class="fieldset-legend">Title</legend>
                        <input
                            v-model="content.title"
                            type="text"
                            class="input"
                            placeholder="Title"
                            @keyup="updateSlug()"
                        />
                    </fieldset>

                    <fieldset class="fieldset w-80">
                        <legend class="fieldset-legend">Slug</legend>
                        <input v-model="content.slug" type="text" class="input" placeholder="Slug" />
                    </fieldset>
                </div>

                <div class="mt-7 flex join flex-none">
                    <select
                        v-model="content.status"
                        class="btn max-w-40 join-item border-0 appearance-none"
                        :class="{
                            'text-success': content.status === 'published',
                            'text-base-content/50': content.status === 'archived',
                        }"
                    >
                        <option v-for="s in status" :key="s" :value="s">{{ s }}</option>
                    </select>
                    <button class="btn join-item" :class="{ 'btn-primary': needsSave }">Save</button>
                </div>
            </div>

            <!-- Toolbar -->
            <div class="join border-t border-s border-e border-base-content/25 rounded-t bg-base-200 mt-4 flex-none">
                <select v-model="format" class="select max-w-40 join-item border-0" @change="heading">
                    <option v-for="head in headings" :key="head.value" :value="head.value">{{ head.name }}</option>
                </select>
                <button v-for="eb in editorButtons" :key="eb.id" class="btn join-item leading-0" @click="eb.func">
                    <i class="bi" :class="eb.icon"></i>
                </button>
            </div>

            <!-- Textarea fills rest -->
            <div class="flex-1 flex">
                <textarea
                    ref="textareaRef"
                    v-model="content.body"
                    class="textarea resize-none w-full rounded-t-none h-full focus:outline-0! focus:border-base-content/60"
                    @keydown="keyHandler"
                    @click="textPosition"
                ></textarea>
            </div>
        </div>
    </div>
</template>
