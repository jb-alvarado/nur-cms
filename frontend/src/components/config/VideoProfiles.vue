<script setup lang="ts">
import { computed, ref } from 'vue'
import { cloneDeep } from 'es-toolkit/object'
import { useI18n } from 'vue-i18n'
import { useIndex } from '@/stores/index'
import { authFetch } from '@/composables/authFetch'
import type { VideoProfile, VideoProfileArg } from '@/types/models.d'
import type { RespondObj } from '@/types/query.d'

import GenericModal from '@/components/generic/GenericModal.vue'

type ProfileDraft = Pick<VideoProfile, 'id' | 'name' | 'container' | 'height' | 'enabled' | 'sort_order'> & {
    cmd: VideoProfileArg[]
}
type SelectableProfile = ProfileDraft & { check: boolean }

const { t } = useI18n()
const store = useIndex()

const profiles = ref<SelectableProfile[]>([])
const select = ref(false)
const selectCount = computed(() => profiles.value.reduce((count, item) => count + (item.check ? 1 : 0), 0))
const ordering = ref('sort_order')
const profile = ref<ProfileDraft>(emptyProfile())

const deleteModal = ref()
const profileModal = ref()
const isEditing = ref(false)

const profileRows = computed<Array<{ name: string; field: 'name' | 'container' | 'height' | 'enabled' }>>(() => [
    { name: t('common.name'), field: 'name' },
    { name: t('videoProfiles.container'), field: 'container' },
    { name: t('videoProfiles.height'), field: 'height' },
    { name: t('videoProfiles.enabled'), field: 'enabled' },
])

function emptyProfile(): ProfileDraft {
    return { id: 0, name: '', container: 'mp4', height: 720, enabled: true, sort_order: 0, cmd: [] }
}

async function selectProfiles() {
    try {
        const response = await authFetch<RespondObj<VideoProfile>>(
            `/api/configuration/video-profiles?ordering=${ordering.value}`,
        )
        profiles.value = response.results.map((item) => ({ ...item, check: false, cmd: item.cmd ?? [] }))
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}

selectProfiles()

function selectAll() {
    for (const item of profiles.value) item.check = select.value
}

function addArg() {
    profile.value.cmd.push({ flag: '', value: '' })
}

function removeArg(index: number) {
    profile.value.cmd.splice(index, 1)
}

/** Splits pasted raw ffmpeg arguments (e.g. `-c:v libx264 -crf 23`) into flag/value rows. */
function parseFfmpegArgs(text: string): VideoProfileArg[] {
    const tokens = text.match(/"[^"]*"|'[^']*'|[^\s]+/g) ?? []
    const cleaned = tokens.map((token) => token.replace(/^["']|["']$/g, ''))
    const rows: VideoProfileArg[] = []

    for (let i = 0; i < cleaned.length; i++) {
        if (!cleaned[i].startsWith('-')) continue
        const flag = cleaned[i]
        const next = cleaned[i + 1]
        const value = next && !next.startsWith('-') ? next : ''
        if (value) i++
        rows.push({ flag, value })
    }

    return rows
}

function handleCmdPaste(event: ClipboardEvent) {
    const pasted = event.clipboardData?.getData('text')
    if (!pasted) return

    event.preventDefault()
    const rows = parseFfmpegArgs(pasted)
    if (rows.length === 0) {
        store.msgAlert('error', String(t('videoProfiles.pasteFailed')))
        return
    }
    profile.value.cmd.push(...rows)
}

function editProfileByIndex(index: number) {
    const item = profiles.value[index]
    if (!item) return

    profile.value = {
        id: item.id,
        name: item.name,
        container: item.container,
        height: item.height,
        enabled: item.enabled,
        sort_order: item.sort_order,
        cmd: cloneDeep(item.cmd),
    }
    isEditing.value = true
    profileModal.value.showModal()
}

function openCreateModal() {
    profile.value = emptyProfile()
    isEditing.value = false
    profileModal.value.showModal()
}

async function deleteProfile() {
    for (const item of profiles.value) {
        if (!item.check) continue
        try {
            await authFetch(`/api/configuration/video-profiles/${item.id}`, { method: 'DELETE' })
            store.msgAlert('success', `Deleted: ${item.name}`)
        } catch (e) {
            store.msgAlert('error', String(e))
        }
    }

    await selectProfiles()
}

function deselect() {
    for (const item of profiles.value) item.check = false
}

async function saveProfile() {
    const url = isEditing.value
        ? `/api/configuration/video-profiles/${profile.value.id}`
        : '/api/configuration/video-profiles'
    const method = isEditing.value ? 'PUT' : 'POST'

    try {
        await authFetch(url, {
            method,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                name: profile.value.name,
                container: profile.value.container,
                height: profile.value.height,
                enabled: profile.value.enabled,
                sort_order: profile.value.sort_order,
                cmd: profile.value.cmd.filter((arg) => arg.flag.trim() !== ''),
            }),
        })
        store.msgAlert('success', `${isEditing.value ? 'Updated' : 'Created'} video profile: ${profile.value.name}`)
        profileModal.value.close()
        await selectProfiles()
    } catch (e) {
        store.msgAlert('error', String(e))
    }
}
</script>

<template>
    <div class="bg-base-200 p-2 border border-base-content/25 rounded-sm w-full md:w-auto">
        <div class="flex">
            <div class="grow font-bold">{{ $t('videoProfiles.title') }}</div>
            <button class="btn btn-sm btn-primary text-base" @click="openCreateModal">{{ $t('button.new') }}</button>
        </div>

        <div class="h-10 flex mt-2 items-center">
            <div class="grow join">
                <div v-if="selectCount > 0">
                    <button class="btn text-warning join-item" @click="deleteModal.showModal()">
                        {{ $t('common.delete') }}
                    </button>
                    <span class="ms-2">{{ selectCount }} {{ $t('common.selected') }}</span>
                </div>
            </div>
        </div>

        <div class="overflow-x-auto mt-4 max-h-96">
            <table class="table bg-base-300 table-pin-rows table-zebra [&_td]:py-2 rounded-sm">
                <thead>
                    <tr>
                        <th class="w-10">
                            <input v-model="select" type="checkbox" class="checkbox checkbox-sm" @change="selectAll" />
                        </th>
                        <th v-for="row in profileRows" :key="row.field" class="min-w-16">{{ row.name }}</th>
                        <th class="w-10"></th>
                    </tr>
                </thead>
                <tbody>
                    <tr v-for="(col, i) in profiles" :key="i">
                        <th><input v-model="col.check" type="checkbox" class="checkbox checkbox-sm" /></th>
                        <td v-for="row in profileRows" :key="row.field">
                            <input
                                v-if="row.field === 'enabled'"
                                type="checkbox"
                                class="checkbox checkbox-sm"
                                :checked="col.enabled"
                                disabled
                            />
                            <template v-else>{{ col[row.field] }}</template>
                        </td>
                        <td>
                            <button class="btn btn-sm p-1" @click="editProfileByIndex(i)">
                                <i class="bi bi-pencil-square text-lg"></i>
                            </button>
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>
    </div>

    <GenericModal
        ref="deleteModal"
        :title="$t('dialog.deleteTitle')"
        :cancel-action="deselect"
        :ok-action="deleteProfile"
    >
        <p class="py-4">{{ $t('dialog.deleteConfirm', { count: selectCount }) }}</p>
    </GenericModal>
    <GenericModal
        ref="profileModal"
        :title="isEditing ? $t('dialog.editVideoProfile') : $t('dialog.createVideoProfile')"
        :ok-action="saveProfile"
    >
        <div class="flex flex-col gap-4">
            <div class="flex gap-2">
                <fieldset class="fieldset py-0 grow">
                    <legend class="fieldset-legend">{{ $t('common.name') }}</legend>
                    <input v-model="profile.name" type="text" class="input w-full" :placeholder="$t('common.name')" />
                </fieldset>
                <fieldset class="fieldset py-0">
                    <legend class="fieldset-legend">{{ $t('videoProfiles.container') }}</legend>
                    <input v-model="profile.container" type="text" class="input w-24" />
                </fieldset>
                <fieldset class="fieldset py-0">
                    <legend class="fieldset-legend">{{ $t('videoProfiles.height') }}</legend>
                    <input v-model.number="profile.height" type="number" min="1" class="input w-24" />
                </fieldset>
            </div>

            <fieldset class="fieldset py-0">
                <label class="label cursor-pointer w-fit gap-2">
                    <input v-model="profile.enabled" type="checkbox" class="checkbox" />
                    <span>{{ $t('videoProfiles.enabled') }}</span>
                </label>
            </fieldset>

            <div class="flex flex-col gap-2">
                <div class="flex items-center">
                    <h3 class="font-semibold grow">{{ $t('videoProfiles.cmd') }}</h3>
                    <button class="btn btn-sm" @click="addArg"><i class="bi bi-plus-lg"></i></button>
                </div>

                <div v-for="(arg, index) in profile.cmd" :key="index" class="flex items-center gap-1">
                    <input
                        v-model="arg.flag"
                        type="text"
                        class="input input-sm w-1/3 font-mono"
                        placeholder="-c:v"
                        :aria-label="$t('videoProfiles.flag')"
                    />
                    <input
                        v-model="arg.value"
                        type="text"
                        class="input input-sm grow font-mono"
                        placeholder="libx264"
                        :aria-label="$t('common.value')"
                    />
                    <button class="btn btn-sm" @click="removeArg(index)"><i class="bi bi-x-lg"></i></button>
                </div>

                <div
                    class="border border-dashed border-base-content/30 rounded-sm p-3 text-sm text-base-content/50 text-center"
                    tabindex="0"
                    @paste="handleCmdPaste"
                >
                    {{ $t('videoProfiles.pasteHint') }}
                </div>
            </div>
        </div>
    </GenericModal>
</template>
