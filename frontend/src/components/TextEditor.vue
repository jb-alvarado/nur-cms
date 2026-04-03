<script setup lang="ts">
import { type ModelRef, nextTick, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'

import GenericModal from '@/components/generic/GenericModal.vue'
import MediaBrowser from '@/components/media/MediaBrowser.vue'

const { t } = useI18n()
const model: ModelRef<string | undefined> = defineModel()
const store = useIndex()

const textareaRef = ref()
const linkModal = ref()
const mediaModal = ref()
const linkName = ref('')
const linkURL = ref('https://')
const lastBody = ref('')
const lastPos = ref(0)
const format = ref(0)

const headings = [
    { name: t('editor.text'), value: 0 },
    { name: t('editor.h2'), value: 2 },
    { name: t('editor.h3'), value: 3 },
    { name: t('editor.h4'), value: 4 },
    { name: t('editor.h5'), value: 5 },
]

const editorButtons = [
    { id: 0, icon: 'bi-type-bold', title: t('editor.bold'), func: bold },
    { id: 1, icon: 'bi-type-italic', title: t('editor.italic'), func: italic },
    { id: 2, icon: 'bi-type-underline', title: t('editor.underline'), func: underline },
    { id: 3, icon: 'bi-type-strikethrough', title: t('editor.strikethrough'), func: strikethrough },
    { id: 4, icon: 'bi-link-45deg', title: t('editor.link'), func: openLinkModal },
    { id: 5, icon: 'bi-image', title: t('editor.image'), func: openMediaBrowser },
    { id: 6, icon: 'bi-quote', title: t('editor.quote'), func: quote },
    { id: 7, icon: 'bi-table', title: t('editor.table'), func: table },
]

const props = defineProps({
    update: {
        type: Function,
        default() {},
    },
    removeNode: {
        type: [Function, null] as const,
        default: null,
    },
})

function openLinkModal() {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd

    linkName.value = textarea.value.substring(start, end)
    linkModal.value.showModal()
}

function openMediaBrowser() {
    mediaModal.value.showModal()
}

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
    const selected = textarea.value.substring(start, end)

    lastBody.value = model.value ?? ''
    lastPos.value = start

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

function clearLink() {
    linkName.value = ''
    linkURL.value = 'https://'
}

function addLink() {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const value = model.value ?? ''

    lastBody.value = model.value ?? ''
    lastPos.value = start

    const linkMarkdown = `[${linkName.value}](${linkURL.value})`
    model.value = value.slice(0, start) + linkMarkdown + value.slice(end)

    nextTick(() => {
        const cursorPos = start + linkMarkdown.length
        textarea.setSelectionRange(cursorPos, cursorPos)
        textarea.focus()

        linkName.value = ''
        linkURL.value = 'https://'
    })
}

function addMedia(m: Media) {
    const alt = m.alt ?? m.filename
    const path = `${m.path}/${m.filename}`

    insertMarkdown(`![${alt}](${path})`)

    mediaModal.value.close()
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

function table() {
    const textarea = textareaRef.value
    if (!textarea) return

    const start = textarea.selectionStart
    const end = textarea.selectionEnd
    const value = textarea.value

    const firstHeader = t('editor.tableHeader1')
    const secondHeader = t('editor.tableHeader2')
    const firstCell = t('editor.tableCell1')
    const secondCell = t('editor.tableCell2')
    const tableMarkdown = `| ${firstHeader} | ${secondHeader} |\n| --- | --- |\n| ${firstCell} | ${secondCell} |`

    const prefix = start > 0 && value[start - 1] !== '\n' ? '\n' : ''
    const suffix = end < value.length && value[end] !== '\n' ? '\n' : ''

    lastBody.value = model.value ?? ''
    lastPos.value = start

    model.value = value.slice(0, start) + prefix + tableMarkdown + suffix + value.slice(end)

    nextTick(() => {
        const headerStart = start + prefix.length + 2
        const headerEnd = headerStart + firstHeader.length
        textarea.setSelectionRange(headerStart, headerEnd)
        textarea.focus()
    })
}
</script>
<template>
    <div class="flex flex-col">
        <div class="join border-t border-s border-e border-base-content/25 rounded-t bg-base-200 mt-2 flex-none">
            <select v-model="format" class="select max-w-40 join-item border-0" @change="heading">
                <option v-for="head in headings" :key="head.value" :value="head.value">{{ head.name }}</option>
            </select>
            <button v-for="eb in editorButtons" :key="eb.id" class="btn join-item leading-0" :title="eb.title" @click="eb.func">
                <i class="bi" :class="eb.icon"></i>
            </button>

            <div class="grow flex justify-end">
                <div class="join">
                    <button class="join-item btn rounded p-3 hidden 2xl:flex" @click="store.preview = !store.preview">
                        <i class="bi bi-markdown scale-130"></i>
                    </button>
                    <button v-if="removeNode" class="join-item btn rounded p-3" @click="removeNode()">
                        <i class="bi bi-x-lg"></i>
                    </button>
                </div>
            </div>
        </div>

        <!-- Textarea grows naturally so resized height pushes following content -->
        <div>
            <textarea
                ref="textareaRef"
                v-model="model"
                class="textarea w-full min-h-50 rounded-t-none focus:outline-0! focus:border-base-content/60 resize-y"
                @keydown="keyHandler"
                @click="textPosition"
            ></textarea>
        </div>

        <GenericModal ref="linkModal" :title="$t('dialog.linkTitle')" :cancel-action="clearLink" :ok-action="addLink">
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('dialog.linkName') }}</legend>
                <input v-model="linkName" type="text" class="input w-full" placeholder="Name" />
            </fieldset>
            <fieldset class="fieldset">
                <legend class="fieldset-legend">{{ $t('dialog.linkURL') }}</legend>
                <input v-model="linkURL" type="text" class="input w-full" placeholder="URL" autofocus />
            </fieldset>
        </GenericModal>

        <MediaBrowser ref="mediaModal" :update="addMedia" />
    </div>
</template>
