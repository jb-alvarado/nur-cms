<script setup lang="ts">
import { ref, watch } from 'vue'
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
import 'dayjs/locale/de'
import 'dayjs/locale/en'
import { RouterLink, useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import { closeDropdown } from '@/utils/helper'

dayjs.extend(localizedFormat)

const route = useRoute()
const store = useIndex()
const { locale } = useI18n()
const groupedColumns = ref<any[]>([])

// Set dayjs locale based on i18n locale
watch(
    () => locale.value,
    (newLocale) => {
        dayjs.locale(newLocale)
    },
    { immediate: true }
)

const props = defineProps({
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
    prefix: {
        type: String,
        default: '',
    },

})

watch(
    () => store.tableCols,
    (newVal) => {
        groupedColumns.value = groupColumns(newVal)
    },
    { deep: true, immediate: true }
)

function groupColumns(columns: any[]) {
    const groups = new Map<string, { items: any[]; firstIndex: number }>()

    // 1. Create groups while preserving the first-seen order
    columns.forEach((item, index) => {
        const key = String(item.group_id ?? item.id) // if no group_id exists -> its own group
        const entry = groups.get(key)

        if (entry) {
            entry.items.push(item)
        } else {
            groups.set(key, { items: [item], firstIndex: index })
        }
    })

    const result = []

    const orderedGroups = Array.from(groups.values()).sort(
        (a, b) => a.firstIndex - b.firstIndex
    )

    // 2. Show only the object with the smallest id AND add an array of all IDs in the group
    for (const { items: group } of orderedGroups) {

        if (group?.length === 1) {
            // no group, just include as-is
            result.push({
                ...group[0],
                locale_ids: group[0].locale_id ? [{ id: group[0].id, locale_id: group[0].locale_id }] : [],
            })
        } else {
            // group → determine the object with the smallest ID
            const sortedGroup = [...(group ?? [])].sort((a, b) => a.locale_id - b.locale_id) // sort by ID for minObj
            const minObj = sortedGroup[0]

            // sort locale_ids by id or locale_id
            const sortedLocaleIds = sortedGroup
                .map((g) => ({ id: g.id, locale_id: g.locale_id }))
                .sort((a, b) => {
                    if (a.locale_id < b.locale_id) return -1
                    if (a.locale_id > b.locale_id) return 1
                    return 0
                })

            result.push({
                ...minObj,
                locale_ids: sortedLocaleIds,
            })
        }
    }

    return result
}

function selectAll() {
    for (const item of store.tableCols) {
        item.check = store.selectAll
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
    for (const r of store.visibleRows) {
        if (r.field !== row.field) {
            r.active = false
        }
    }

    row.active = true
    store.ordering = row.up ? row.field : `-${row.field}`
    store.contentSelect()
}

function onChangeCheckbox() {
    const colMap = new Map<number, any>()
    for (const col of store.tableCols) {
        if (col.id !== null && col.id !== undefined) {
            colMap.set(col.id, col)
        }
    }

    for (const item of groupedColumns.value) {
        const mainCol = colMap.get(item.id)
        if (mainCol) mainCol.check = item.check

        for (const loc of item.locale_ids) {
            const locCol = colMap.get(loc.id)
            if (locCol) locCol.check = loc.check ?? item.check
        }
    }

    props.checkBoxChange()
}
</script>

<template>
    <table class="table bg-base-300 table-zebra [&_td]:py-2 rounded-sm">
        <thead>
            <tr>
                <th class="w-10">
                    <label>
                        <input v-model="store.selectAll" type="checkbox" class="checkbox checkbox-sm" @change="selectAll" />
                    </label>
                </th>
                <th
                    v-for="row in store.visibleRows.filter(
                        (r) =>
                            r.field !== 'locale_id' &&
                            r.field !== 'group_id' &&
                            (route.params.type === 'event' || (r.field !== 'start_time' && r.field !== 'end_time'))
                    )"
                    :key="row.field"
                    :class="{ 'w-16': row.field === 'id' }"
                >
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
                <th v-if="type !== 'author' && type !== 'comment'">{{ $t('common.languages') }}</th>
                <th class="w-10"></th>
            </tr>
        </thead>
        <tbody>
            <tr v-for="(col, i) in groupedColumns" :key="i">
                <th>
                    <label>
                        <input
                            v-model="col.check"
                            type="checkbox"
                            class="checkbox checkbox-sm"
                            @change="onChangeCheckbox()"
                        />
                    </label>
                </th>
                <td
                    v-for="row in store.visibleRows.filter(
                        (r) =>
                            r.field !== 'locale_id' &&
                            r.field !== 'group_id' &&
                            (route.params.type === 'event' || (r.field !== 'start_time' && r.field !== 'end_time'))
                    )"
                    :key="row.field"
                >
                    <span v-if="col[row.field] === 'published'" class="text-success bg-base-100 p-1 rounded border">
                        {{ $t('status.published') }}
                    </span>
                    <span v-else-if="col[row.field] === 'approved'" class="text-success bg-base-100 p-1 rounded border">
                        {{ $t('status.approved') }}
                    </span>
                    <span v-else-if="col[row.field] === 'draft'" class="bg-base-100 p-1 rounded border">
                        {{ $t('status.draft') }}
                    </span>
                    <span v-else-if="col[row.field] === 'pending'" class="bg-base-100 p-1 rounded border">
                        {{ $t('status.pending') }}
                    </span>
                    <span
                        v-else-if="col[row.field] === 'archived'"
                        class="bg-base-100 p-1 rounded border text-base-content/60 border-base-content/60"
                    >
                        {{ $t('status.archived') }}
                    </span>
                    <span
                        v-else-if="col[row.field] === 'rejected'"
                        class="text-error bg-base-100 p-1 rounded border border-error/60"
                    >
                        {{ $t('status.rejected') }}
                    </span>
                    <span v-else>
                        {{ formatField(col, row.field) }}
                    </span>
                </td>
                <td v-if="col.locale_ids.length > 0">
                    <details
                        v-if="col.locale_ids.length > 1"
                        class="dropdown"
                        :class="{ 'dropdown-top': groupedColumns.length - 3 < i }"
                    >
                        <summary class="btn font-normal" @blur="closeDropdown">
                            {{ store.locales.find((l) => l.id === col.locale_id)?.name }}
                        </summary>
                        <ul class="menu dropdown-content bg-base-100 rounded-box z-1 w-28 p-0 shadow-sm">
                            <li v-for="lo in col.locale_ids" :key="lo.id">
                                <RouterLink :to="`${prefix}/${type}/${lo.id}`" class="rounded-box">
                                    {{ store.locales.find((l) => l.id === lo.locale_id)?.name }}
                                </RouterLink>
                            </li>
                        </ul>
                    </details>
                    <div class="btn font-normal btn-disabled text-base-content" v-else>
                        {{ store.locales.find((l) => l.id === col.locale_id)?.name }}
                    </div>
                </td>
                <td>
                    <RouterLink :to="`${prefix}/${type}/${col.id}`" class="btn btn-sm p-1">
                        <i class="bi bi-pencil-square text-lg"></i>
                    </RouterLink>
                </td>
            </tr>
        </tbody>
    </table>
</template>
