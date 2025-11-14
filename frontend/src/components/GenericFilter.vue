<script setup lang="ts">
import { useRoute } from 'vue-router'
import { useIndex } from '@/stores/index'

const route = useRoute()
const store = useIndex()

defineProps({
    hideFields: {
        type: Boolean,
        default: false,
    },
})
</script>

<template>
    <div class="join">
        <button
            class="btn border border-base-content/20 join-item"
            @click="store.contentSelect(null, store.previous)"
            :disabled="!store.previous"
        >
            <i class="bi bi-caret-left"></i>
        </button>
        <button
            class="btn border border-base-content/20 join-item"
            @click="store.contentSelect(null, store.next)"
            :disabled="!store.next"
        >
            <i class="bi bi-caret-right"></i>
        </button>
        <select v-model="store.limit" class="select join-item" @change="store.setItemLimit()">
            <option v-for="lim in store.limits" :key="lim" :value="lim">{{ lim }}</option>
        </select>
        <div v-if="!hideFields" class="join-item dropdown dropdown-end border border-base-content/20">
            <div tabindex="0" role="button" class="btn p-2 h-9">
                <i class="bi bi-gear text-lg leading-0"></i>
            </div>
            <ul tabindex="-1" class="dropdown-content menu bg-base-300 rounded-sm z-1 w-52 p-2 mt-1 shadow-sm">
                <li
                    v-for="row in store.allRows.filter(
                        (r) =>
                            r.field !== 'locale_id' &&
                            r.field !== 'group_id' &&
                            (route.params.type === 'event' || (r.field !== 'start_time' && r.field !== 'end_time'))
                    )"
                    :key="row.field"
                >
                    <label>
                        <input
                            v-model="row.check"
                            type="checkbox"
                            class="checkbox checkbox-sm"
                            @change="store.activeFields"
                            :disabled="row.field === 'id'"
                        />
                        {{ row.name }}
                    </label>
                </li>
            </ul>
        </div>
    </div>
</template>
