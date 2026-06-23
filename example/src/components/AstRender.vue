<script setup lang="ts">
import { computed } from 'vue'
import { RouterLink } from 'vue-router'

import { astMediaPath, extractAstText } from '@/utils/helper'
import type { MediaSerializer } from '../../../frontend/src/types/serialized'

defineOptions({ name: 'AstRender' })

interface AstNode {
    type?: string
    text?: string
    html?: string
    value?: string
    code?: boolean
    bold?: boolean
    italic?: boolean
    strikethrough?: boolean
    level?: number
    url?: string
    alt?: string
    title?: string
    path?: string
    filename?: string
    src?: string
    variants?: MediaSerializer['variants']
    identifier?: string
    label?: string
    ordered?: boolean
    children?: AstNode[]
}

const props = withDefaults(
    defineProps<{
        content?: AstNode[] | AstNode | null
        inline?: boolean
        textOnly?: boolean
    }>(),
    {
        content: null,
        inline: false,
        textOnly: false,
    },
)

const nodes = computed<AstNode[]>(() => {
    if (!props.content) return []
    return Array.isArray(props.content) ? props.content : [props.content]
})

const plainText = computed(() => extractAstText(nodes.value).trim())

function children(node: AstNode): AstNode[] {
    return Array.isArray(node.children) ? node.children : []
}

function nodeKey(node: AstNode, index: number): string {
    return String(node.identifier ?? node.url ?? node.text ?? `${node.type ?? 'node'}-${index}`)
}

function headingTag(level?: number): string {
    if (!level || level < 1 || level > 6) return 'h2'
    return `h${level}`
}

function listTag(node: AstNode): string {
    return node.ordered ? 'ol' : 'ul'
}

function tableRows(node: AstNode): AstNode[] {
    return children(node).filter((child) => child.type === 'tableRow' || child.type === 'table_row')
}

function tableCells(row: AstNode): AstNode[] {
    return children(row).filter((child) => child.type === 'tableCell' || child.type === 'table_cell')
}

function cellTag(rowIndex: number): string {
    return rowIndex === 0 ? 'th' : 'td'
}

function textValue(node: AstNode): string {
    return String(node.text ?? node.value ?? '')
}

function htmlValue(node: AstNode): string {
    return String(node.html ?? node.text ?? '')
}

function linkUrl(node: AstNode): string {
    return String(node.url ?? '')
}

function isInternalLink(node: AstNode): boolean {
    const url = linkUrl(node)
    return url.startsWith('/') && !url.startsWith('//')
}

function externalTarget(node: AstNode): string | undefined {
    return isInternalLink(node) ? undefined : '_blank'
}

function externalRel(node: AstNode): string | undefined {
    return isInternalLink(node) ? undefined : 'noopener noreferrer'
}

function imageSrc(node: AstNode): string {
    return astMediaPath(node, 1280)
}

function imageAlt(node: AstNode): string {
    return String(node.alt ?? node.title ?? '')
}

function footnoteId(node: AstNode): string {
    return String(node.identifier ?? node.label ?? '')
}

function footnoteLabel(node: AstNode): string {
    return String(node.label ?? node.identifier ?? '')
}

function isText(node: AstNode): boolean {
    return node.type === 'text' || (!node.type && textValue(node) !== '')
}

function isFootnoteReference(node: AstNode): boolean {
    return node.type === 'footnoteReference' || node.type === 'footnote_reference'
}

function isFootnoteDefinition(node: AstNode): boolean {
    return node.type === 'footnoteDefinition' || node.type === 'footnote_definition'
}

function isThematicBreak(node: AstNode): boolean {
    return node.type === 'thematicBreak' || node.type === 'thematic_break'
}
</script>

<template>
    <component :is="inline ? 'span' : 'div'" class="ast-render">
        <template v-if="textOnly">
            {{ plainText }}
        </template>

        <template v-else>
            <template v-for="(node, index) in nodes" :key="nodeKey(node, index)">
                <component :is="headingTag(node.level)" v-if="node.type === 'heading'">
                    <AstRender :content="children(node)" inline />
                </component>

                <p v-else-if="node.type === 'paragraph'">
                    <AstRender :content="children(node)" inline />
                </p>

                <blockquote v-else-if="node.type === 'blockquote'">
                    <AstRender :content="children(node)" />
                </blockquote>

                <component :is="listTag(node)" v-else-if="node.type === 'list'" class="pl-6">
                    <li v-for="(item, itemIndex) in children(node)" :key="nodeKey(item, itemIndex)">
                        <AstRender :content="children(item)" />
                    </li>
                </component>

                <RouterLink
                    v-else-if="node.type === 'link' && isInternalLink(node)"
                    :to="linkUrl(node)"
                    class="text-primary underline underline-offset-2"
                >
                    <AstRender v-if="children(node).length" :content="children(node)" inline />
                    <span v-else>{{ linkUrl(node) }}</span>
                </RouterLink>

                <a
                    v-else-if="node.type === 'link'"
                    :href="linkUrl(node)"
                    :target="externalTarget(node)"
                    :rel="externalRel(node)"
                    class="text-primary underline underline-offset-2"
                >
                    <AstRender v-if="children(node).length" :content="children(node)" inline />
                    <span v-else>{{ linkUrl(node) }}</span>
                </a>

                <img
                    v-else-if="node.type === 'image' && imageSrc(node)"
                    :src="imageSrc(node)"
                    :alt="imageAlt(node)"
                    class="rounded-lg"
                />

                <div v-else-if="node.type === 'table'" class="overflow-x-auto">
                    <table class="table table-auto min-w-full">
                        <tbody>
                            <tr v-for="(row, rowIndex) in tableRows(node)" :key="nodeKey(row, rowIndex)">
                                <component
                                    :is="cellTag(rowIndex)"
                                    v-for="(cell, cellIndex) in tableCells(row)"
                                    :key="nodeKey(cell, cellIndex)"
                                    :scope="rowIndex === 0 ? 'col' : undefined"
                                    class="border border-base-content/20 p-2 align-top"
                                >
                                    <AstRender :content="children(cell)" inline />
                                </component>
                            </tr>
                        </tbody>
                    </table>
                </div>

                <a
                    v-else-if="isFootnoteReference(node)"
                    :id="`fn-ref${footnoteId(node)}`"
                    :href="`#fn-def${footnoteId(node)}`"
                    role="doc-noteref"
                >
                    <sup>[{{ footnoteLabel(node) }}]</sup>
                </a>

                <div
                    v-else-if="isFootnoteDefinition(node)"
                    :id="`fn-def${footnoteId(node)}`"
                    class="flex items-start gap-2 text-sm"
                    role="doc-endnote"
                >
                    <a :href="`#fn-ref${footnoteId(node)}`" class="shrink-0 font-semibold"
                        >[{{ footnoteLabel(node) }}]</a
                    >
                    <div>
                        <AstRender :content="children(node)" />
                    </div>
                </div>

                <hr v-else-if="isThematicBreak(node)" />

                <br v-else-if="node.type === 'break'" />

                <pre v-else-if="node.type === 'code' || node.type === 'math'"><code>{{ textValue(node) }}</code></pre>

                <code v-else-if="isText(node) && node.code">{{ textValue(node) }}</code>

                <span
                    v-else-if="isText(node)"
                    :class="{
                        'font-bold': node.bold,
                        italic: node.italic,
                        'line-through': node.strikethrough,
                    }"
                >
                    {{ textValue(node) }}
                </span>

                <component
                    :is="inline ? 'span' : 'div'"
                    v-else-if="node.type === 'html'"
                    class="contents"
                    v-html="htmlValue(node)"
                />

                <AstRender v-else-if="children(node).length" :content="children(node)" :inline="inline" />
            </template>
        </template>
    </component>
</template>
