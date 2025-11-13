<script setup lang="ts">
import { ref, type PropType } from 'vue'
import dayjs from 'dayjs'
import { RouterLink, useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'

const emit = defineEmits(['update:ordering'])
const route = useRoute()
const store = useIndex()
const select = ref(false)
const ordering = ref('id')

const props = defineProps({
    columns: {
        type: Array as PropType<any[]>,
        default: [] as any[],
    },
    rows: {
        type: Array as PropType<any[]>,
        default: [] as any[],
    },
    type: {
        type: String,
        default: '',
    },
    checkBoxChange: {
        type: Function,
        default() {
            return ''
        },
    },
})

function selectAll() {
    for (const item of props.columns) {
        item.check = select.value
    }

    props.checkBoxChange()
}

function formatField(col: any, field: string) {
    if (['created_at', 'updated_at'].includes(field)) {
        return dayjs(col[field] as string).format('llll')
    } else if (col['meta'] && (field === 'start_time' || field === 'end_time')) {
        return dayjs(col['meta'][field]).format('llll')
    } else if (field === 'author') {
        return `${col[field]?.first_name} ${col[field]?.last_name}`
    } else {
        return col[field]
    }
}

function orderRows(row: any) {
    for (const r of props.rows) {
        if (r.field !== row.field) {
            r.active = false
        }
    }

    row.active = true
    ordering.value = row.up ? row.field : `-${row.field}`
    emit('update:ordering', ordering.value)

    store.contentSelect()
}
</script>

<template>
    <table class="table bg-base-300 table-zebra [&_td]:py-2 rounded-sm">
        <thead>
            <tr>
                <th class="w-10">
                    <label>
                        <input v-model="select" type="checkbox" class="checkbox checkbox-sm" @change="selectAll" />
                    </label>
                </th>
                <th v-for="row in rows.filter(r => route.params.type === 'event' || (r.field !== 'start_time' && r.field !== 'end_time'))" :key="row.field" :class="{'w-16': row.field === 'id'}">
                    <label class="swap" :class="{ 'text-base-content': row.active }">
                        <input type="checkbox" v-model="row.up" @change="orderRows(row)" />
                        <div class="swap-on">
                            {{ row.name }}
                            <i v-if="row.active" class="bi bi-caret-up-fill"></i>
                        </div>
                        <div class="swap-off">
                            {{ row.name }}
                            <i v-if="row.active" class="bi bi-caret-down-fill"></i>
                        </div>
                    </label>
                </th>
                <th class="w-10"></th>
            </tr>
        </thead>
        <tbody>
            <tr v-for="(col, i) in columns" :key="i">
                <th>
                    <label>
                        <input
                            v-model="col.check"
                            type="checkbox"
                            class="checkbox checkbox-sm"
                            @change="checkBoxChange()"
                        />
                    </label>
                </th>
                <td v-for="row in rows.filter(r => route.params.type === 'event' || (r.field !== 'start_time' && r.field !== 'end_time'))" :key="row.field">
                    <span v-if="col[row.field] === 'published'" class="text-success bg-base-100 p-1 rounded border">
                        {{ formatField(col, row.field) }}
                    </span>
                    <span v-else-if="col[row.field] === 'draft'" class="bg-base-100 p-1 rounded border">
                        {{ formatField(col, row.field) }}
                    </span>
                    <span v-else-if="col[row.field] === 'archived'" class="bg-base-100 p-1 rounded border text-base-content/60 border-base-content/60">
                        {{ formatField(col, row.field) }}
                    </span>
                    <span v-else>
                        {{ formatField(col, row.field) }}
                    </span>
                </td>
                <td>
                    <RouterLink :to="`/${type}/${col.id}`" class="btn btn-sm p-1">
                        <i class="bi bi-pencil-square text-lg"></i>
                    </RouterLink>
                </td>
            </tr>
        </tbody>
    </table>
</template>
