<script setup lang="ts">
import { type ModelRef, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'

const { t } = useI18n()
const model: ModelRef<string | undefined> = defineModel()
const store = useIndex()

const textareaRef = ref()
const lastBody = ref('')
const lastPos = ref(0)
const format = ref(0)

const headings = [
    { name: t('editor.text'), value: 0 },
    { name: t('editor.h1'), value: 1 },
    { name: t('editor.h2'), value: 2 },
    { name: t('editor.h3'), value: 3 },
    { name: t('editor.h4'), value: 4 },
    { name: t('editor.h5'), value: 5 },
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

const props = defineProps({
    update: {
        type: Function,
        default() {},
    },
})

function keyHandler(e: KeyboardEvent) {
    const textarea = textareaRef.value
    if (!textarea) return

    props.update()

    if (e.ctrlKey && e.key === 'z' && lastBody.value) {
        e.preventDefault()

        model.value = lastBody.value

        nextTick(() => {
            textarea.setSelectionRange(lastPos.value, lastPos.value)
            textarea.focus()
        })

        lastBody.value = ''
    } else if (e.key === 'Tab') {
        e.preventDefault()
        const start = textarea.selectionStart
        const end = textarea.selectionEnd
        const value = textarea.value

        const tab = '    '

        lastBody.value = model.value ?? ''
        lastPos.value = start

        if (start === end) {
            model.value = value.slice(0, start) + tab + value.slice(end)
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

            model.value = before + indented + after

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

    lastBody.value = model.value ?? ''
    lastPos.value = start

    if (before.startsWith('[]')) {
        before = before.replace('[]', `[${selected}]`)
        selected = selected.startsWith('http') ? selected : `https://${selected}`
    }

    const value = model.value ?? ''
    let cursorPos = start + before.length

    if (start === end) {
        const newValue = value.slice(0, start) + before + after + value.slice(end)
        model.value = newValue
    } else {
        const newValue = value.slice(0, start) + before + selected + after + value.slice(end)
        model.value = newValue
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

    lastBody.value = model.value ?? ''
    lastPos.value = start

    model.value = value.slice(0, lineStart) + newLine + value.slice(actualLineEnd)

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

    lastBody.value = model.value ?? ''
    lastPos.value = start

    model.value = value.slice(0, lineStart) + newLine + value.slice(actualLineEnd)

    nextTick(() => {
        const cursorPos = lineStart + newLine.length
        textarea.setSelectionRange(cursorPos, cursorPos)
        textarea.focus()
    })
}
</script>
<template>
    <div class="h-full flex flex-col">
        <div class="join border-t border-s border-e border-base-content/25 rounded-t bg-base-200 mt-2 flex-none">
            <select v-model="format" class="select max-w-40 join-item border-0" @change="heading">
                <option v-for="head in headings" :key="head.value" :value="head.value">{{ head.name }}</option>
            </select>
            <button v-for="eb in editorButtons" :key="eb.id" class="btn join-item leading-0" @click="eb.func">
                <i class="bi" :class="eb.icon"></i>
            </button>

            <div class="grow flex justify-end">
                <button class="btn rounded p-3" @click="store.preview = !store.preview">
                    <i class="bi bi-markdown text-xl"></i>
                </button>
            </div>
        </div>

        <!-- Textarea fills rest -->
        <div class="flex-1 flex">
            <textarea
                ref="textareaRef"
                v-model="model"
                class="textarea resize-none w-full h-full rounded-t-none focus:outline-0! focus:border-base-content/60"
                @keydown="keyHandler"
                @click="textPosition"
            ></textarea>
        </div>
    </div>
</template>
