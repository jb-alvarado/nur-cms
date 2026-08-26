<script setup lang="ts">
import { computed, type ModelRef } from 'vue'

type DataField = {
    key: string
    label?: string | null
    kind?: 'string' | 'text' | 'boolean' | 'number' | 'json'
}

const block: ModelRef<Record<string, any> | undefined> = defineModel('block')
const props = withDefaults(
    defineProps<{
        schema?: DataField[]
    }>(),
    { schema: () => [] },
)

function inferredKind(value: unknown): NonNullable<DataField['kind']> {
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
            <textarea v-if="field.kind === 'text'" v-model="block[field.key]" rows="3" class="textarea grow"></textarea>
            <input v-else-if="field.kind === 'boolean'" v-model="block[field.key]" type="checkbox" class="checkbox" />
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
            <input v-else v-model="block[field.key]" type="text" class="input grow" />
        </div>
    </div>
</template>
