<script setup lang="ts">
import { useIndex } from '@/stores/index'

const store = useIndex()

defineProps({
    hideFields: {
        type: Boolean,
        default: false,
    },
})
</script>

<template>
    <div class="dropdown dropdown-end rounded border border-base-content/20">
        <div tabindex="0" role="button" class="btn p-2.5">
            <i class="bi bi-gear text-lg leading-0"></i>
        </div>
        <ul tabindex="-1" class="dropdown-content menu bg-base-300 rounded-sm z-1 w-52 p-2 mt-1 shadow-sm">
            <template v-for="row in store.allRows" :key="row.field">
                <li
                    v-if="
                        row.field !== 'locale_id' &&
                        row.field !== 'group_id' &&
                        ((row.field !== 'start_time' && row.field !== 'end_time') || store.type.use_meta)
                    "
                >
                    <label>
                        <input
                            v-model="row.check"
                            type="checkbox"
                            class="checkbox checkbox-sm"
                            @change="store.activeFields()"
                            :disabled="row.field === 'id'"
                        />
                        {{ row.name }}
                    </label>
                </li>
            </template>
        </ul>
    </div>
</template>
