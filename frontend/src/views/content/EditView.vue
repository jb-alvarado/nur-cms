<script setup lang="ts">
import { ref, nextTick, computed } from 'vue'
import { useRoute, RouterLink } from 'vue-router'
import { cloneDeep, isEqual } from 'lodash-es'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'
import { errMsg } from '@/utils/error'
import { slugify } from '@/utils/slugify.js'

const route = useRoute()
const contentId = Number(route.params.id ?? 0)
const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type
const groupID = BigInt(Number(route.params.group_id ?? 0))

const auth = useAuth()
const store = useIndex()
const content = ref({
    id: 0,
    type: '',
    title: '',
    slug: '',
    description: '',
    body: '',
    status: 'draft',
    locale_id: 0,
    group_id: groupID,
    group_members: [],
    check: false,
} as Content)
const contentOriginal = ref(cloneDeep(content))
const locales = ref<Locale[]>([])
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

if (contentId > 0) {
    getContent()
} else if (groupID > 0) {
    fetch(`/api/content/entries/${typeParam}?group_id=${groupID}&fields=group_members&output_type=markdown`, {
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
        .then((response: RespondObj) => {
            const groupMemberLocaleIds = new Set(
                response.results.flatMap(
                    (result: RespondObj) => result.group_members?.map((member: GroupMember) => member.locale_id) ?? []
                )
            )
            locales.value = store.locales.filter((locale) => !groupMemberLocaleIds.has(locale.id))
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
} else {
    setTimeout(() => {
        locales.value = store.locales
    }, 1000)
}

function getContent() {
    fetch(`/api/content/entries/${typeParam}?id=${contentId}&output_type=markdown`, {
        headers: auth.authHeader,
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }

            return resp.json()
        })
        .then((response: RespondObj) => {
            content.value = response.results[0]
            contentOriginal.value = cloneDeep(content.value)

            locales.value = store.locales.filter((locale) => {
                const isCurrentLocale = locale.id === content.value.locale_id
                const hasGroupMember = content.value.group_members?.some((member) => member.locale_id === locale.id)
                return isCurrentLocale || hasGroupMember
            })
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
}

function updateSlug() {
    if (content.value.title) {
        content.value.slug = slugify(content.value.title)
    }
}

function updateDescription() {
    if (!content.value.body) return

    const bodyWithoutFrontmatter = content.value.body
        .split('\n')
        .filter((line: string) => {
            const trimmed = line.trim()
            return (
                !trimmed.startsWith('#') &&
                !trimmed.startsWith('![') &&
                !trimmed.startsWith('[') &&
                !trimmed.startsWith('<') &&
                !trimmed.startsWith('>')
            )
        })
        .join('\n')
        .trim()

    const excerpt = bodyWithoutFrontmatter.slice(0, 255)

    if (!content.value.description && excerpt.length > 160) {
        content.value.description = excerpt
    }
}

function keyHandler(e: KeyboardEvent) {
    const textarea = textareaRef.value
    if (!textarea) return

    updateDescription()

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

function memberLink(id: number): string {
    const member = content.value.group_members?.find((member) => member.locale_id === id)

    return `/${typeParam}/${member?.id ?? content.value.id}`
}

function closeDropdown(event: Event) {
    const target = event.target as HTMLElement

    setTimeout(() => {
        ;(target.parentNode as HTMLElement | null)?.removeAttribute('open')
    }, 170)
}

function save() {
    const payload = Object.fromEntries(
        Object.entries(content.value).filter(([key, value]) => {
            return !isEqual(value, contentOriginal.value[key as keyof Content])
        })
    )

    if (Object.keys(payload).length === 0) {
        store.msgAlert('warning', 'No changes to save', 3)
        return
    }

    fetch(`/api/content/entries${contentId > 0 ? `/${contentId}` : ''}`, {
        method: contentId > 0 ? 'PUT' : 'POST',
        headers: {
            ...auth.authHeader,
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(payload),
    })
        .then(async (resp) => {
            if (resp.status >= 400) {
                const msg = await errMsg(resp)
                throw new Error(msg)
            }
            store.msgAlert('success', 'Content saved successfully', 3)
            getContent()
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
}
</script>

<template>
    <div class="flex flex-col h-full pb-6">
        <div class="flex-none">
            <h1 class="text-2xl h-8">{{ content?.title ?? '' }}</h1>
        </div>

        <!-- Form + Editor Container -->
        <div
            v-if="content"
            class="flex flex-col flex-1 2xl:w-2/3 min-h-96 bg-base-300 p-4 pt-1 mt-4 rounded overflow-hidden"
        >
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

                <div class="mt-7 flex gap-2 flex-none">
                    <div class="join">
                        <details v-if="content.id === 0" class="dropdown">
                            <summary class="btn join-item" @blur="closeDropdown">
                                {{ store.locales.find((l) => l.id === content.locale_id)?.name || 'Locale' }}
                            </summary>
                            <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-2 shadow-sm">
                                <li v-for="l in locales" :key="l.id">
                                    <a @click="content.locale_id = l.id">{{ l.name }}</a>
                                </li>
                            </ul>
                        </details>

                        <details v-if="(content.id ?? 0) > 0" class="dropdown">
                            <summary class="btn join-item" @blur="closeDropdown">
                                {{ store.locales.find((l) => l.id === content.locale_id)?.name }}
                            </summary>
                            <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-34 p-2 shadow-sm">
                                <li v-for="l in locales" :key="l.id">
                                    <RouterLink :to="memberLink(l.id!)">{{ l.name }}</RouterLink>
                                </li>
                            </ul>
                        </details>

                        <RouterLink
                            :to="`/${typeParam}/0/${content.group_id}`"
                            class="btn join-item px-2"
                            title="Add Language"
                        >
                            <i class="bi bi-plus-lg"></i>
                        </RouterLink>

                        <button class="btn btn-disabled bg-base-300 p-1"></button>

                        <details class="dropdown">
                            <summary
                                class="btn join-item"
                                :class="{
                                    'text-success': content.status === 'published',
                                    'text-base-content/50': content.status === 'archived',
                                }"
                                @blur="closeDropdown"
                            >
                                {{ content.status }}
                            </summary>
                            <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-24 p-2 shadow-sm">
                                <li
                                    v-for="s in status"
                                    :key="s"
                                    :class="{
                                        'text-base-content/50': content.status !== s,
                                    }"
                                >
                                    <a @click="content.status = s">{{ s }}</a>
                                </li>
                            </ul>
                        </details>
                    </div>

                    <button class="btn" :class="{ 'btn-primary': needsSave }" @click="save()">Save</button>
                </div>
            </div>

            <div class="w-full">
                <fieldset class="fieldset">
                    <legend class="fieldset-legend">Description</legend>
                    <textarea
                        v-model="content.description"
                        class="textarea h-20 w-full"
                        placeholder="Description"
                    ></textarea>
                </fieldset>
            </div>

            <!-- Toolbar -->
            <div class="join border-t border-s border-e border-base-content/25 rounded-t bg-base-200 mt-2 flex-none">
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
                    class="textarea resize-none w-full h-full rounded-t-none focus:outline-0! focus:border-base-content/60"
                    @keydown="keyHandler"
                    @click="textPosition"
                ></textarea>
            </div>
        </div>
    </div>
</template>
