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
                    <a v-if="child.type === 'link'" :href="String(child.url)">
                        <template v-for="linkChild in child.children">
                            <span v-if="linkChild.type === 'text'">{{ linkChild.text }}</span>
                        </template>
                    </a>
                    <span v-else-if="child.type === 'text'">
                        {{ child.text }}
                    </span>
                </template>
            </h1>
            <h2 v-else-if="ast.type === 'heading' && ast.level === 2">
                <template v-for="child in ast.children">
                    <a v-if="child.type === 'link'" :href="String(child.url)">
                        <template v-for="linkChild in child.children">
                            <span v-if="linkChild.type === 'text'">{{ linkChild.text }}</span>
                        </template>
                    </a>
                    <span v-else-if="child.type === 'text'">
                        {{ child.text }}
                    </span>
                </template>
            </h2>
            <h3 v-else-if="ast.type === 'heading' && ast.level === 2">
                <template v-for="child in ast.children">
                    <a v-if="child.type === 'link'" :href="String(child.url)">
                        <template v-for="linkChild in child.children">
                            <span v-if="linkChild.type === 'text'">{{ linkChild.text }}</span>
                        </template>
                    </a>
                    <span v-else-if="child.type === 'text'">
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
                    <a v-else-if="child.type === 'link'" :href="String(child.url)">
                        <template v-for="linkChild in child.children">
                            <img v-if="linkChild.type === 'image'" :src="String(linkChild.src || `${linkChild.path}/${linkChild.filename}`)" :alt="String(linkChild.alt || '')" />
                            <strong v-else-if="linkChild.bold">{{ linkChild.text }}</strong>
                            <i v-else-if="linkChild.italic">{{ linkChild.text }}</i>
                            <span v-else>{{ linkChild.text }}</span>
                        </template>
                    </a>
                    <img
                        v-else-if="child.type === 'image'"
                        :src="String(child.src || `${child.path}/${child.filename}`)"
                        :alt="String(child.alt || '')"
                    />
                    <template v-else-if="child.type === 'text'">
                        {{ child.text }}
                    </template>
                </template>
            </p>
            <img v-else-if="ast.type === 'image'" :src="String(ast.src || `${ast.path}/${ast.filename}`)" :alt="String(ast.alt || '')" />
            <a v-else-if="ast.type === 'link'" :href="String(ast.url)">
                <template v-for="linkChild in ast.children">
                    <img v-if="linkChild.type === 'image'" :src="String(linkChild.src || `${linkChild.path}/${linkChild.filename}`)" :alt="String(linkChild.alt || '')" />
                    <strong v-else-if="linkChild.bold">{{ linkChild.text }}</strong>
                    <i v-else-if="linkChild.italic">{{ linkChild.text }}</i>
                    <span v-else>{{ linkChild.text }}</span>
                </template>
            </a>
            <div v-else-if="ast.type === 'html'" v-html="ast.text" />
        </template>
    </div>
</template>
