<script setup lang="ts">
import { computed, ref, nextTick, watch, type ModelRef, type PropType } from 'vue'

const limit: ModelRef<number | undefined> = defineModel('limit')
const offset: ModelRef<number | undefined> = defineModel('offset')

const props = defineProps({
    total: { type: Number, required: true },
    maxButtons: { type: Number, default: 3 },
    hideStat: {type: Boolean, default: false},
    pageSizes: { type: Array as PropType<number[]>, default: () => [5, 10, 25, 50, 100] },
})

const emit = defineEmits(['change'])

// normalize limit; avoid 0 or negative
const effectiveLimit = computed(() => (limit.value! > 0 ? Math.floor(limit.value!) : 10))
const totalPages = computed(() =>
    effectiveLimit.value > 0 ? Math.max(1, Math.ceil(props.total / effectiveLimit.value)) : 1
)

const currentPage = computed(() => {
    if (effectiveLimit.value <= 0) return 1
    return Math.min(totalPages.value, Math.floor(offset.value! / effectiveLimit.value) + 1)
})

const isFirst = computed(() => currentPage.value <= 1)
const isLast = computed(() => currentPage.value >= totalPages.value)

// display ranges: from/to for current page
const displayFrom = computed(() => {
    if (props.total === 0) return 0
    return offset.value! + 1
})
const displayTo = computed(() => {
    return Math.min(props.total, offset.value! + effectiveLimit.value)
})

// page button list generation (with ellipses)
const pages = computed(() => {
    const max = Math.max(3, props.maxButtons | 0) // ensure >=3
    const total = totalPages.value
    const current = currentPage.value
    if (total <= max) {
        return Array.from({ length: total }, (_, i) => i + 1)
    }

    const half = Math.floor(max / 2)
    let start = Math.max(1, current - half)
    let end = Math.min(total, current + half)

    if (start === 1) {
        end = Math.min(total, start + max - 1)
    } else if (end === total) {
        start = Math.max(1, end - max + 1)
    } else {
        // ensure window size
        const size = end - start + 1
        if (size < max) {
            if (start > 1) start = Math.max(1, start - (max - size))
            else end = Math.min(total, end + (max - size))
        }
    }

    const list = []
    if (start > 1) {
        list.push(1)
        if (start > 2) list.push('...')
    }
    for (let p = start; p <= end; p++) list.push(p)
    if (end < total) {
        if (end < total - 1) list.push('...')
        list.push(total)
    }
    return list
})

// jump input
const jumpPage = ref(currentPage.value)
watch(currentPage, (v) => (jumpPage.value = v))

// helpers to change page/limit
function setPage(page: number) {
    const clamped = Math.max(1, Math.min(totalPages.value, Math.floor(page)))
    const newOffset = (clamped - 1) * effectiveLimit.value

    limit.value = effectiveLimit.value
    offset.value = newOffset
    emitUpdate(clamped)
}

function goto(page: number) {
    setPage(page)
}

function prev() {
    if (!isFirst.value) setPage(currentPage.value - 1)
}

function next() {
    if (!isLast.value) setPage(currentPage.value + 1)
}

function onLimitChange() {
    nextTick(() => {
        const newLimit = Math.max(1, Number(limit.value) || effectiveLimit.value)
        const itemIndex = Math.floor(offset.value!)
        const newPage = Math.floor(itemIndex / newLimit) + 1
        const newOffset = (newPage - 1) * newLimit

        limit.value = newLimit
        offset.value = newOffset
        emitUpdate(newPage)
    })
}

function emitUpdate(pageVal: number) {
    emit('change', { page: pageVal })
}
</script>

<template>
    <div v-if="!hideStat" class="text-sm text-base-content/70 me-4 mt-1.3 leading-0">{{ displayFrom }}–{{ displayTo }} of {{ total }}</div>
    <nav class="flex join" aria-label="Pagination">
        <!-- prev -->
        <button class="btn join-item border-base-content/20" :disabled="isFirst" @click="prev">Prev</button>

        <!-- page numbers -->
        <template v-for="p in pages" :key="p">
            <button
                v-if="p !== '...'"
                class="btn join-item border-base-content/20"
                :class="{ 'btn-disabled': p === currentPage }"
                @click="goto(Number(p))"
            >
                {{ p }}
            </button>
            <button v-else class="btn btn-disabled join-item border-t-base-content/20 border-b-base-content/20">…</button>
        </template>

        <!-- next -->
        <button class="btn join-item border border-base-content/20" :disabled="isLast" @click="next">Next</button>
        <!-- page size selector -->
        <select class="select join-item min-w-14" v-model="limit" @change="onLimitChange">
            <option v-for="s in pageSizes" :key="s" :value="s">
                {{ s }}
            </option>
        </select>
    </nav>
</template>
