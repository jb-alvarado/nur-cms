<script setup lang="ts">
interface AstNode {
    type: string
    children?: AstNode[]
    [key: string]: unknown
}

defineProps({
    content: {
        type: Array<AstNode>,
        default: [],
    },
})
</script>
<template>
    <div class="mt-8 prose">
        <template v-for="(ast, i) in content" :key="i">
            <h1 v-if="ast.type === 'heading' && ast.level === 1">
                <template v-for="child in ast.children">
                    <span v-if="child.type === 'text'">
                        {{ child.text }}
                    </span>
                </template>
            </h1>
            <h2 v-else-if="ast.type === 'heading' && ast.level === 2">
                <template v-for="child in ast.children">
                    <span v-if="child.type === 'text'">
                        {{ child.text }}
                    </span>
                </template>
            </h2>
            <h3 v-else-if="ast.type === 'heading' && ast.level === 2">
                <template v-for="child in ast.children">
                    <span v-if="child.type === 'text'">
                        {{ child.text }}
                    </span>
                </template>
            </h3>
            <p v-else-if="ast.type === 'paragraph'">
                <template v-for="child in ast.children">
                    <strong v-if="child.bold">
                        {{ child.text }}
                    </strong>
                    <i v-else-if="child.italic">
                        {{ child.text }}
                    </i>
                    <img
                        v-else-if="child.type === 'image'"
                        :src="`${child.path}/${child.filename}`"
                        :alt="String(child.alt || '')"
                    />
                    <template v-else-if="child.type === 'text'">
                        {{ child.text }}
                    </template>
                </template>
            </p>
            <img v-else-if="ast.type === 'image'" :src="`${ast.path}/${ast.filename}`" :alt="String(ast.alt || '')" />
            <div v-else-if="ast.type === 'html'" v-html="ast.text" />
        </template>
    </div>
</template>
