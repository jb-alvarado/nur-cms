<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'

import { renderMarkdownPreview } from '@/composables/markdownPreview'

const { t } = useI18n()
const modal = ref<HTMLDialogElement | null>(null)
const previews = reactive(new Map<string, string>())
let previewController: AbortController | undefined

type MarkdownHelpItem = {
    id: string
    label: string
    syntax: string
}

const sections = computed(
    () =>
        [
            {
                title: t('markdownHelp.basic'),
                items: [
                    {
                        id: 'heading',
                        label: t('markdownHelp.heading'),
                        syntax: `## ${t('markdownHelp.examples.heading')}`,
                    },
                    { id: 'bold', label: t('markdownHelp.bold'), syntax: `**${t('markdownHelp.examples.bold')}**` },
                    { id: 'italic', label: t('markdownHelp.italic'), syntax: `*${t('markdownHelp.examples.italic')}*` },
                    { id: 'code', label: t('markdownHelp.code'), syntax: '`code`' },
                    {
                        id: 'link',
                        label: t('markdownHelp.link'),
                        syntax: `[${t('markdownHelp.examples.link')}](https://example.com)`,
                    },
                    {
                        id: 'image',
                        label: t('markdownHelp.image'),
                        syntax: `![${t('markdownHelp.examples.imageAlt')}](/admin/logo.png)`,
                    },
                    {
                        id: 'list',
                        label: t('markdownHelp.list'),
                        syntax: `- ${t('markdownHelp.examples.first')}\n- ${t('markdownHelp.examples.second')}`,
                    },
                    { id: 'quote', label: t('markdownHelp.quote'), syntax: `> ${t('markdownHelp.examples.quote')}` },
                    { id: 'code-block', label: t('markdownHelp.codeBlock'), syntax: '```\ncode\n```' },
                    { id: 'rule', label: t('markdownHelp.rule'), syntax: '---' },
                ],
            },
            {
                title: t('markdownHelp.gfm'),
                items: [
                    {
                        id: 'strikethrough',
                        label: t('markdownHelp.strikethrough'),
                        syntax: `~~${t('markdownHelp.examples.deleted')}~~`,
                    },
                    { id: 'table', label: t('markdownHelp.table'), syntax: '| A | B |\n| - | - |\n| 1 | 2 |' },
                    { id: 'task', label: t('markdownHelp.task'), syntax: `- [x] ${t('markdownHelp.examples.done')}` },
                    { id: 'autolink', label: t('markdownHelp.autolink'), syntax: 'https://example.com' },
                    {
                        id: 'footnote',
                        label: t('markdownHelp.footnote'),
                        syntax: `${t('markdownHelp.examples.text')}[^1]\n\n[^1]: ${t('markdownHelp.examples.note')}`,
                    },
                    {
                        id: 'inline-footnote',
                        label: t('markdownHelp.inlineFootnote'),
                        syntax: `${t('markdownHelp.examples.text')}^[${t('markdownHelp.examples.note')}]`,
                    },
                ],
            },
            {
                title: t('markdownHelp.extensions'),
                items: [
                    {
                        id: 'underline',
                        label: t('markdownHelp.underline'),
                        syntax: `__${t('markdownHelp.examples.underlined')}__`,
                    },
                    {
                        id: 'highlight',
                        label: t('markdownHelp.highlight'),
                        syntax: `==${t('markdownHelp.examples.highlighted')}==`,
                    },
                    {
                        id: 'insert',
                        label: t('markdownHelp.insert'),
                        syntax: `++${t('markdownHelp.examples.added')}++`,
                    },
                    { id: 'superscript', label: t('markdownHelp.superscript'), syntax: 'x^2^' },
                    {
                        id: 'subtext',
                        label: t('markdownHelp.subtext'),
                        syntax: `-# ${t('markdownHelp.examples.subtext')}`,
                    },
                    {
                        id: 'spoiler',
                        label: t('markdownHelp.spoiler'),
                        syntax: `||${t('markdownHelp.examples.spoiler')}||`,
                    },
                    {
                        id: 'multiline-quote',
                        label: t('markdownHelp.multilineQuote'),
                        syntax: `>>>\n${t('markdownHelp.examples.quote')}\n>>>`,
                    },
                    {
                        id: 'alert',
                        label: t('markdownHelp.alert'),
                        syntax: `> [!WARNING]\n> ${t('markdownHelp.examples.alert')}`,
                    },
                    {
                        id: 'directive',
                        label: t('markdownHelp.directive'),
                        syntax: `:::notice\n${t('markdownHelp.examples.directive')}\n:::`,
                    },
                ],
            },
        ] satisfies Array<{ title: string; items: MarkdownHelpItem[] }>,
)

async function loadPreviews() {
    previewController?.abort()
    const controller = new AbortController()
    previewController = controller
    const items = sections.value.flatMap((section) => section.items)

    try {
        const response = await renderMarkdownPreview(
            items.map((item) => ({ key: item.id, markdown: item.syntax })),
            controller.signal,
        )
        if (controller.signal.aborted) return

        previews.clear()
        for (const node of response.nodes) previews.set(node.key, node.html)
    } catch (error) {
        if (!(error instanceof DOMException && error.name === 'AbortError')) {
            console.error('Markdown help preview failed', error)
        }
    }
}

function showModal() {
    modal.value?.showModal()
    loadPreviews()
}

onBeforeUnmount(() => previewController?.abort())

defineExpose({ showModal })
</script>

<template>
    <dialog ref="modal" class="modal modal-bottom sm:modal-middle">
        <div class="modal-box max-w-4xl max-h-[calc(100vh-4rem)]">
            <form method="dialog">
                <button
                    class="btn btn-sm btn-circle btn-ghost absolute right-2 top-2"
                    :aria-label="$t('common.cancel')"
                >
                    <i class="bi bi-x-lg"></i>
                </button>
            </form>
            <h2 class="text-lg font-bold">{{ $t('markdownHelp.title') }}</h2>

            <section v-for="section in sections" :key="section.title" class="mt-4">
                <h3 class="font-semibold">{{ section.title }}</h3>
                <div class="mt-2 grid gap-2 sm:grid-cols-2">
                    <article
                        v-for="item in section.items"
                        :key="item.id"
                        class="rounded border border-base-content/15 p-2 text-sm"
                    >
                        <p class="font-medium">{{ item.label }}</p>
                        <code class="mt-1 block overflow-x-auto rounded bg-base-200 p-1 text-xs whitespace-pre-wrap">{{
                            item.syntax
                        }}</code>
                        <div
                            class="mt-2 min-h-6 border-t border-base-content/10 pt-2"
                            v-html="previews.get(item.id) ?? ''"
                        ></div>
                    </article>
                </div>
            </section>
        </div>
        <form method="dialog" class="modal-backdrop">
            <button>{{ $t('common.cancel') }}</button>
        </form>
    </dialog>
</template>
