<script setup lang="ts">
import { ref } from 'vue'
import { useRoute } from 'vue-router'
import MarkdownRender from 'vue-renderer-markdown'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const route = useRoute()

const auth = useAuth()
const store = useIndex()
const article = ref({} as Content)

fetch(`/api/content/article/?id=${route.params.id}&output_type=markdown`, {
    headers: auth.authHeader,
})
    .then(async (resp) => {
        if (resp.status >= 400) {
            const msg = (await resp.json())?.error ?? (await resp.text())
            throw new Error(msg)
        }
        return resp.json()
    })
    .then((response: RespondObj) => {
        article.value = response.results[0]

    })
    .catch((e) => {
        store.msgAlert('error', e, 6)
    })
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ article.title }}</h1>
        </div>
        <div class="flex gap-8">
        <textarea v-model="article.body" class="textarea w-full xl:w-1/2"></textarea>
        <MarkdownRender :content="article.body" class="prose hidden xl:block w-1/2 bg-base-200 p-4 rounded" />
        </div>

    </div>
</template>
