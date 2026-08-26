<script setup lang="ts">
import { computed, type ModelRef } from 'vue'
import type { ContentNodeDataField, ContentNodeDataKind } from '@/types/models.d'
import type { JsonValue } from '@/types/serde_json/JsonValue'

type DataField = Pick<ContentNodeDataField, 'key' | 'label' | 'kind'>

const block: ModelRef<Record<string, JsonValue> | undefined> = defineModel('block')
const props = withDefaults(
    defineProps<{
        schema?: DataField[]
    }>(),
    { schema: () => [] },
)

function inferredKind(value: unknown): ContentNodeDataKind {
    if (typeof value === 'boolean') return 'boolean'
    if (typeof value === 'number') return 'number'
    if (value !== null && typeof value === 'object') return 'json'
    return 'string'
}

const fields = computed<DataField[]>(() => {
    const values = block.value ?? {}
    const schemaKeys = new Set(props.schema.map((field) => field.key))
    const extraFields = Object.keys(values)
        .filter((key) => !schemaKeys.has(key))
        .map((key) => ({ key, kind: inferredKind(values[key]) }))

    return [...props.schema, ...extraFields]
})

function updateNumber(key: string, event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value
    if (block.value) block.value[key] = value === '' ? 0 : Number(value)
}

function updateString(key: string, event: Event) {
    if (block.value) block.value[key] = (event.currentTarget as HTMLInputElement).value
}

function updateBoolean(key: string, event: Event) {
    if (block.value) block.value[key] = (event.currentTarget as HTMLInputElement).checked
}

function updateJson(key: string, event: Event) {
    if (!block.value) return

    const input = event.currentTarget as HTMLTextAreaElement
    try {
        block.value[key] = JSON.parse(input.value)
        input.setCustomValidity('')
    } catch {
        input.setCustomValidity('Invalid JSON')
        input.reportValidity()
    }
}

function jsonValue(value: unknown): string {
    return JSON.stringify(value ?? null, null, 2)
}
</script>

<template>
    <div v-if="block" class="flex flex-col gap-2">
        <div
            v-for="field in fields"
            :key="field.key"
            class="flex gap-2"
            :class="field.kind === 'text' || field.kind === 'json' ? 'items-start' : 'items-center'"
        >
            <label class="min-w-20" :class="{ 'pt-3': field.kind === 'text' || field.kind === 'json' }">
                {{ field.label || field.key }}:
            </label>
            <textarea
                v-if="field.kind === 'text'"
                :value="String(block[field.key] ?? '')"
                rows="3"
                class="textarea grow"
                @input="updateString(field.key, $event)"
            ></textarea>
            <input
                v-else-if="field.kind === 'boolean'"
                :checked="block[field.key] === true"
                type="checkbox"
                class="checkbox"
                @change="updateBoolean(field.key, $event)"
            />
            <input
                v-else-if="field.kind === 'number'"
                :value="block[field.key]"
                type="number"
                class="input grow"
                @input="updateNumber(field.key, $event)"
            />
            <textarea
                v-else-if="field.kind === 'json'"
                :value="jsonValue(block[field.key])"
                rows="4"
                class="textarea grow font-mono"
                @change="updateJson(field.key, $event)"
            ></textarea>
            <input
                v-else
                :value="String(block[field.key] ?? '')"
                type="text"
                class="input grow"
                @input="updateString(field.key, $event)"
            />
        </div>
    </div>
</template>
