<script setup lang="ts">
import { computed, ref } from 'vue'
import { cloneDeep } from 'es-toolkit/object'
import { useI18n } from 'vue-i18n'
import { authFetch } from '@/composables/authFetch'
import { useIndex } from '@/stores/index'
import { mediaPath } from '@/utils/helper'
import { locales as adminLocales } from '@/i18n'
import { entryEditorFieldDefinitions, entryEditorStatuses } from '@/config/editor-settings'

import MediaBrowser from '@/components/media/MediaBrowser.vue'

const { t } = useI18n()
const store = useIndex()
const configuration = ref<CmsConfiguration>()
const mediaModal = ref<InstanceType<typeof MediaBrowser>>()
const logoPreview = ref<string | null>(null)
const entryEditorFields = computed(() =>
    entryEditorFieldDefinitions.map((field) => ({ ...field, label: t(field.label) })),
)

const menuItems = computed(() => [
    { id: 'authors', label: t('button.author') },
    { id: 'categories', label: t('button.category') },
    ...store.types
        .filter((type) => type.slug)
        .map((type) => ({ id: `content:${type.slug}`, label: type.name ?? type.slug ?? '' })),
    { id: 'media', label: t('button.media') },
])

const commentsEnabled = computed({
    get: () => !configuration.value?.disabled_features.includes('comments'),
    set: (enabled: boolean) => {
        if (!configuration.value) return
        configuration.value.disabled_features = enabled
            ? configuration.value.disabled_features.filter((feature) => feature !== 'comments')
            : [...new Set([...configuration.value.disabled_features, 'comments'])]
    },
})

function menuItemVisible(id: string): boolean {
    return !configuration.value?.hidden_menu_items.includes(id)
}

function setMenuItemVisible(id: string, visible: boolean) {
    if (!configuration.value) return
    configuration.value.hidden_menu_items = visible
        ? configuration.value.hidden_menu_items.filter((item) => item !== id)
        : [...new Set([...configuration.value.hidden_menu_items, id])]
}

function entryFieldVisible(id: string): boolean {
    return !configuration.value?.entry_hidden_fields.includes(id)
}

function setEntryFieldVisible(id: string, visible: boolean) {
    if (!configuration.value) return
    configuration.value.entry_hidden_fields = visible
        ? configuration.value.entry_hidden_fields.filter((field) => field !== id)
        : [...new Set([...configuration.value.entry_hidden_fields, id])]
}

async function selectConfiguration() {
    try {
        const selected = await authFetch<CmsConfiguration>('/api/configuration/cms')
        configuration.value = cloneDeep(selected)
        store.cmsConfiguration = selected
        store.cmsConfigurationLoaded = true
        await store.selectBranding(true)
        logoPreview.value = store.branding.logo_url
    } catch (error) {
        store.msgAlert('error', error instanceof Error ? error.message : String(error))
    }
}

async function updateConfiguration() {
    if (!configuration.value) return

    try {
        configuration.value.frontend_name = configuration.value.frontend_name.trim()
        await authFetch('/api/configuration/cms', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(configuration.value),
        })
        store.cmsConfiguration = cloneDeep(configuration.value)
        store.cmsConfigurationLoaded = true
        await store.selectBranding(true)
        logoPreview.value = store.branding.logo_url
        store.msgAlert('success', t('cmsSettings.updated'))
    } catch (error) {
        store.msgAlert('error', error instanceof Error ? error.message : String(error))
    }
}

function selectLogo(media: Media) {
    if (!configuration.value || media.id == null || !media.type?.startsWith('image/')) return

    configuration.value.logo_media_id = media.id
    logoPreview.value = mediaPath(media)
    mediaModal.value?.close()
}

function clearLogo() {
    if (!configuration.value) return
    configuration.value.logo_media_id = null
    logoPreview.value = null
}

selectConfiguration()
</script>

<template>
    <div v-if="configuration" class="bg-base-200 p-2 border border-base-content/25 rounded-sm max-w-xl w-full">
        <div class="flex mb-4">
            <div class="grow font-bold">{{ $t('cmsSettings.title') }}</div>
            <button class="btn btn-sm btn-primary text-base" @click="updateConfiguration()">
                {{ $t('button.save') }}
            </button>
        </div>

        <div class="grid gap-4">
            <label class="form-control">
                <span class="label-text mb-1 font-semibold">{{ $t('cmsSettings.frontendName') }}</span>
                <input
                    v-model="configuration.frontend_name"
                    type="text"
                    maxlength="160"
                    class="input input-bordered w-full"
                />
            </label>

            <label class="form-control">
                <span class="label-text mb-1 font-semibold">{{ $t('cmsSettings.adminLanguage') }}</span>
                <select v-model="configuration.admin_language" class="select select-bordered w-full">
                    <option :value="null">{{ $t('cmsSettings.adminLanguageAutomatic') }}</option>
                    <option v-for="language in adminLocales" :key="language.code" :value="language.language">
                        {{ language.name }}
                    </option>
                </select>
                <span class="mt-1 text-sm text-base-content/70">{{ $t('cmsSettings.adminLanguageHelp') }}</span>
            </label>

            <div>
                <div class="mb-2 font-semibold">{{ $t('cmsSettings.logo') }}</div>
                <div class="flex flex-wrap items-center gap-3">
                    <img
                        v-if="logoPreview"
                        :src="logoPreview"
                        :alt="configuration.frontend_name"
                        class="size-20 rounded-sm border border-base-content/20 object-contain"
                    />
                    <div class="join">
                        <button class="btn btn-sm join-item" @click="mediaModal?.showModal()">
                            {{ $t('cmsSettings.selectLogo') }}
                        </button>
                        <button
                            v-if="configuration.logo_media_id !== null"
                            class="btn btn-sm join-item"
                            @click="clearLogo()"
                        >
                            {{ $t('cmsSettings.clearLogo') }}
                        </button>
                    </div>
                </div>
            </div>
        </div>

        <div class="mt-5 border-t border-base-content/20 pt-4">
            <h2 class="font-bold">{{ $t('cmsSettings.entryEditor') }}</h2>
            <label class="mt-3 form-control">
                <span class="label-text mb-1 font-semibold">{{ $t('cmsSettings.defaultEntryStatus') }}</span>
                <select v-model="configuration.entry_default_status" class="select select-bordered w-full">
                    <option v-for="status in entryEditorStatuses" :key="status" :value="status">
                        {{ $t(`status.${status}`) }}
                    </option>
                </select>
            </label>
            <p class="mt-4 text-sm text-base-content/70">{{ $t('cmsSettings.hiddenEntryFieldsHelp') }}</p>
            <div class="mt-2 grid gap-2 sm:grid-cols-2">
                <label
                    v-for="field in entryEditorFields"
                    :key="field.id"
                    class="flex cursor-pointer items-center gap-2"
                >
                    <input
                        :checked="entryFieldVisible(field.id)"
                        type="checkbox"
                        class="checkbox checkbox-sm"
                        @change="setEntryFieldVisible(field.id, ($event.target as HTMLInputElement).checked)"
                    />
                    <span>{{ field.label }}</span>
                </label>
            </div>
        </div>

        <div class="mt-5 border-t border-base-content/20 pt-4">
            <h2 class="font-bold">{{ $t('cmsSettings.features') }}</h2>
            <label class="mt-3 flex cursor-pointer items-center justify-between gap-4">
                <span>
                    <span class="block font-semibold">{{ $t('cmsSettings.comments') }}</span>
                    <span class="block text-sm text-base-content/70">{{ $t('cmsSettings.commentsHelp') }}</span>
                </span>
                <input v-model="commentsEnabled" type="checkbox" class="toggle toggle-primary" />
            </label>

            <div class="mt-5 border-t border-base-content/20 pt-4">
                <h2 class="font-bold">{{ $t('cmsSettings.menuEntries') }}</h2>
                <p class="mt-1 text-sm text-base-content/70">{{ $t('cmsSettings.menuEntriesHelp') }}</p>
                <div class="mt-3 grid gap-2 sm:grid-cols-2">
                    <label v-for="item in menuItems" :key="item.id" class="flex cursor-pointer items-center gap-2">
                        <input
                            :checked="menuItemVisible(item.id)"
                            type="checkbox"
                            class="checkbox checkbox-sm"
                            @change="setMenuItemVisible(item.id, ($event.target as HTMLInputElement).checked)"
                        />
                        <span>{{ item.label }}</span>
                    </label>
                </div>
            </div>
        </div>
    </div>

    <MediaBrowser ref="mediaModal" :update="selectLogo" :media-types="['image']" />
</template>
