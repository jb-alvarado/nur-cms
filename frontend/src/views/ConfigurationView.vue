<script setup lang="ts">
import { ref, computed } from 'vue'
import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

const auth = useAuth()
const store = useIndex()

const types = ref<ContentTypeExt[]>([])
const select = ref(false)
const selectCount = computed(() => types.value.reduce((acc, item: any) => acc + (item.check ? 1 : 0), 0))
const ordering = ref('id')

const typeRows = ref([
    { active: false, up: false, name: 'ID', field: 'id' },
    { active: false, up: false, name: 'Name', field: 'name' },
    { active: false, up: false, name: 'Slug', field: 'slug' },
])

async function typeSelect() {
    await fetch(`/api/content/types?ordering=${ordering.value}`, {
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
            if (response.results?.length > 0) {
                types.value = response.results.map((o: any) => ({ check: false, ...o }))
            } else {
                types.value = []
            }
        })
        .catch((e) => {
            store.msgAlert('error', e, 6)
        })
}

typeSelect()

function selectAll() {
    for (const item of types.value) {
        item.check = select.value
    }
}

function orderRows(row: any) {
    for (const r of types.value) {
        if (r.field !== row.field) {
            r.active = false
        }
    }

    row.active = true
    ordering.value = row.up ? row.field : `-${row.field}`

    typeSelect()
}

async function deleteType() {
    for (const item of types.value) {
        if (item.check) {
            await fetch(`/api/content/types/${item.id}`, {
                method: 'DELETE',
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const json = await resp.json()
                        const msg = json ? json.error : await resp.text()
                        store.msgAlert('error', msg, 6)
                    } else {
                        store.msgAlert('success', `Deleted: ${item.name ?? item.id}`, 2)
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e, 6)
                })
        }
    }

    await typeSelect()
}
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl">{{ $t('button.configure') }}</h1>
        </div>
    </div>

    <div class="h-10 mt-4 mb-6 flex items-center">
        <div class="grow join">
            <div v-if="selectCount > 0">
                <button class="btn text-warning join-item" onclick="delete_modal.showModal()">Delete</button>
                <span class="ms-2">{{ selectCount }} Selected</span>
            </div>
        </div>
    </div>

    <div class="overflow-x-auto mt-4 w-96">
        <table class="table bg-base-300 table-zebra [&_td]:py-2 rounded-sm">
            <thead>
                <tr>
                    <th>
                        <label>
                            <input v-model="select" type="checkbox" class="checkbox checkbox-sm" @change="selectAll" />
                        </label>
                    </th>
                    <th v-for="row in typeRows" :key="row.field" class="min-w-16">
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
                    <th class="w-20"></th>
                </tr>
            </thead>
            <tbody>
                <tr v-for="(col, i) in types" :key="i">
                    <th>
                        <label>
                            <input
                                v-model="col.check"
                                type="checkbox"
                                class="checkbox checkbox-sm"
                            />
                        </label>
                    </th>
                    <td v-for="row in typeRows" :key="row.field">
                        {{ (col as any)[row.field] }}
                    </td>
                    <td>
                        <RouterLink :to="`/article/${col.id}`" class="btn btn-sm p-1">
                            <i class="bi bi-pencil-square text-lg"></i>
                        </RouterLink>
                    </td>
                </tr>
            </tbody>
        </table>
    </div>
    <dialog id="delete_modal" class="modal modal-bottom sm:modal-middle">
        <div class="modal-box">
            <h3 class="text-lg font-bold">Delete Selection</h3>
            <p class="py-4">Are you sure you want to delete this article{{ selectCount > 1 ? 's' : '' }}?</p>
            <div class="modal-action">
                <form method="dialog">
                    <button class="btn">Cancel</button>
                    <button class="btn" @click="deleteType()">Ok</button>
                </form>
            </div>
        </div>
    </dialog>
</template>
