<script setup lang="ts">
import { ref } from 'vue'
import { useRoute } from 'vue-router'
import MarkdownRender from 'vue-renderer-markdown'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const route = useRoute()
const typeParam = Array.isArray(route.params.type) ? route.params.type[0] : route.params.type

const auth = useAuth()
const store = useIndex()
const content = ref({} as Content)

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
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ content.title }}</h1>
        </div>
        <div class="flex gap-8">
            <textarea v-model="content.body" class="textarea w-full xl:w-1/2"></textarea>
            <MarkdownRender :content="content.body" class="prose hidden xl:block w-1/2 bg-base-200 p-4 rounded" />
        </div>
    </div>
</template>
