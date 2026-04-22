<script setup lang="ts">
import { watch } from 'vue'
import { sortBy } from 'es-toolkit/array'
import dayjs from 'dayjs'
import localizedFormat from 'dayjs/plugin/localizedFormat'
import 'dayjs/locale/de'
import 'dayjs/locale/en'
import { RouterLink } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'

dayjs.extend(localizedFormat)

const store = useIndex()
const { locale } = useI18n()

// Set dayjs locale based on i18n locale
watch(
    () => locale.value,
    (newLocale) => {
        dayjs.locale(newLocale)
    },
    { immediate: true },
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
    } else if (field === 'author.first_name,author.last_name') {
        if (Array.isArray(col.authors) && col.authors.length > 0) {
            return col.authors
                .map((author: any) => `${author?.first_name ?? ''} ${author?.last_name ?? ''}`.trim())
                .filter((name: string) => name.length > 0)
                .join(', ')
        }

        return ''
    } else {
        return col[field]
    }
}

function orderRows(row: any) {
    for (const r of store.visibleRows) {
        r.active = false
    }

    row.active = true
    store.ordering = row.up ? row.field : `-${row.field}`
    store.saveTableState()
    store.contentSelect()
}

function onChangeCheckbox() {
    const colMap = new Map<number, any>()
    for (const col of store.tableCols) {
        if (col.id !== null && col.id !== undefined) {
            colMap.set(col.id, col)
        }
    }

    // for (const item of groupedColumns.value) {
    //     const mainCol = colMap.get(item.id)
    //     if (mainCol) mainCol.check = item.check

    //     for (const loc of item.locale_ids) {
    //         const locCol = colMap.get(loc.id)
    //         if (locCol) locCol.check = loc.check ?? item.check
    //     }
    // }

    props.checkBoxChange()
}

function getValue(col: any, row: { field: string }) {
    return col?.[row.field]
}
</script>

<template>
    <table class="table bg-base-300 table-zebra [&_td]:py-2 rounded-sm">
        <thead>
            <tr>
                <th class="w-10">
                    <label>
                        <input
                            v-model="store.selectAll"
                            type="checkbox"
                            class="checkbox checkbox-sm"
                            @change="selectAll"
                        />
                    </label>
                </th>
                <th
                    v-for="row in store.visibleRows.filter(
                        (r) =>
                            r.field !== 'locale_id' &&
                            r.field !== 'group_id' &&
                            (store.routeType === 'event' || (r.field !== 'start_time' && r.field !== 'end_time')),
                    )"
                    :key="row.field"
                    :class="{ 'w-16': row.field === 'id' }"
                    :style="row.minWidth ? { minWidth: `${row.minWidth}px` } : {}"
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
            <tr v-for="(col, i) in store.tableCols" :key="i">
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
                            (store.routeType === 'event' || (r.field !== 'start_time' && r.field !== 'end_time')),
                    )"
                    :key="row.field"
                >
                    <span v-if="getValue(col, row) === 'published'" class="text-success bg-base-100 p-1 rounded border">
                        {{ $t('status.published') }}
                    </span>
                    <span
                        v-else-if="getValue(col, row) === 'approved'"
                        class="text-success bg-base-100 p-1 rounded border"
                    >
                        {{ $t('status.approved') }}
                    </span>
                    <span v-else-if="getValue(col, row) === 'draft'" class="bg-base-100 p-1 rounded border">
                        {{ $t('status.draft') }}
                    </span>
                    <span v-else-if="getValue(col, row) === 'pending'" class="bg-base-100 p-1 rounded border">
                        {{ $t('status.pending') }}
                    </span>
                    <span
                        v-else-if="getValue(col, row) === 'archived'"
                        class="bg-base-100 p-1 rounded border text-base-content/60 border-base-content/60"
                    >
                        {{ $t('status.archived') }}
                    </span>
                    <span
                        v-else-if="getValue(col, row) === 'rejected'"
                        class="text-error bg-base-100 p-1 rounded border border-error/60"
                    >
                        {{ $t('status.rejected') }}
                    </span>
                    <RouterLink
                        v-else-if="['author_name', 'title', 'name', 'first_name', 'last_name'].includes(row.field)"
                        :to="`${prefix}/${type}/${col.id}`"
                        class="hover:text-base-content/70"
                    >
                        {{ formatField(col, row.field) }}
                    </RouterLink>
                    <span v-else :class="{ 'line-clamp-1': row.field === 'text' }">
                        {{ formatField(col, row.field) }}
                    </span>
                </td>
                <td v-if="col.locale_id">
                    <template v-if="Array.isArray(col.group_members) && col.group_members.length > 0">
                        <button
                            class="btn btn-sm btn-primary text-base font-normal"
                            :popovertarget="`lang-select-${col.id}`"
                            :style="`anchor-name: --anchor-${i}`"
                        >
                            {{ store.locales.find((l) => l.id === col.locale_id)?.name }}
                        </button>
                        <ul
                            class="dropdown menu w-28 rounded-box bg-base-100 shadow-sm p-0"
                            :class="{ 'dropdown-top': i > 2 && store.tableCols.length - 2 < i }"
                            popover
                            :id="`lang-select-${col.id}`"
                            :style="`position-anchor: --anchor-${i}`"
                        >
                            <li v-for="lo in sortBy(col.group_members, ['locale_name'])" :key="lo.id">
                                <RouterLink :to="`${prefix}/${type}/${lo.id}`" class="rounded-box">
                                    {{ lo.locale_name }}
                                </RouterLink>
                            </li>
                        </ul>
                    </template>
                    <div class="btn btn-sm text-base font-normal btn-disabled text-base-content" v-else>
                        {{ store.locales.find((l) => l.id === col.locale_id)?.name }}
                    </div>
                </td>
                <td>
                    <RouterLink :to="`${prefix}/${type}/${col.id}`" class="btn btn-sm hover:text-base-content/70 p-1">
                        <i class="bi bi-pencil-square text-lg"></i>
                    </RouterLink>
                </td>
            </tr>
        </tbody>
    </table>
</template>
