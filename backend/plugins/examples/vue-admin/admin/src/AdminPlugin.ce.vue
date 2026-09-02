<script setup lang="ts">
import { ref } from 'vue'

import type { PluginAdminContext } from './plugin-context'

const props = defineProps<{
    context?: PluginAdminContext
}>()

const loading = ref(false)
const result = ref('No request made yet.')
const succeeded = ref<boolean>()

async function testRequest() {
    if (!props.context) {
        succeeded.value = false
        result.value = 'The nur-cms plugin context is unavailable.'
        return
    }

    loading.value = true
    succeeded.value = undefined
    result.value = 'Loading…'
    try {
        const response = await props.context.request('/api/plugins/vue-admin/ping')
        const message = await response.text()
        if (!response.ok) {
            throw new Error(message || `Request failed with status ${response.status}.`)
        }
        succeeded.value = true
        result.value = message || `Request succeeded with status ${response.status}.`
    } catch (reason: unknown) {
        succeeded.value = false
        result.value = reason instanceof Error ? reason.message : 'The request failed.'
    } finally {
        loading.value = false
    }
}
</script>

<template>
    <section class="card">
        <p class="eyebrow">Example Plugin</p>
        <h1>Hello from Vue!</h1>
        <p>This Vue component was loaded independently at runtime.</p>

        <button type="button" :disabled="loading" @click="testRequest">
            {{ loading ? 'Requesting…' : 'Test API request' }}
        </button>

        <output :class="{ success: succeeded === true, error: succeeded === false }">
            {{ result }}
        </output>
    </section>
</template>

<style>
:host {
    display: block;
    color: #172033;
    font-family: system-ui, sans-serif;
}

.card {
    max-width: 36rem;
    padding: 1.5rem;
    border: 1px solid #d7dce5;
    border-radius: 0.75rem;
    background: #fff;
    box-shadow: 0 0.5rem 1.5rem rgb(23 32 51 / 8%);
}

.eyebrow {
    margin: 0 0 0.35rem;
    color: #526078;
    font-size: 0.75rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
}

h1 {
    margin: 0;
    font-size: 1.6rem;
}

button {
    margin-top: 1rem;
    padding: 0.65rem 1rem;
    border: 0;
    border-radius: 0.45rem;
    background: #3157d5;
    color: #fff;
    cursor: pointer;
    font: inherit;
    font-weight: 650;
}

button:disabled {
    cursor: wait;
    opacity: 0.65;
}

output {
    display: block;
    margin-top: 1rem;
    padding: 0.75rem;
    border-radius: 0.4rem;
    background: #f2f4f8;
    overflow-wrap: anywhere;
}

output.success {
    color: #12613a;
    background: #e7f7ee;
}

output.error {
    color: #9d2525;
    background: #fff0f0;
}
</style>
