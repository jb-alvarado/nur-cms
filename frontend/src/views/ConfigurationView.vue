<script setup lang="ts">
import { ref } from 'vue'
import GlobalSettings from '@/components/config/GlobalSettings.vue'
import ContentLocale from '@/components/config/ContentLocale.vue'
import ContentType from '@/components/config/ContentType.vue'
import MailTargets from '@/components/config/MailTargets.vue'
import NodeTemplates from '@/components/config/NodeTemplates.vue'
import VideoProfiles from '@/components/config/VideoProfiles.vue'
import CmsSettings from '@/components/config/CmsSettings.vue'

const appVersion = __APP_VERSION__
const activeTab = ref<'system' | 'cms'>('system')
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ $t('button.configure') }}</h1>

            <div class="text-xs text-base-content/60">v{{ appVersion }}</div>
        </div>
    </div>

    <div role="tablist" class="tabs tabs-border mt-4">
        <button
            role="tab"
            class="tab"
            :class="{ 'tab-active': activeTab === 'system' }"
            :aria-selected="activeTab === 'system'"
            @click="activeTab = 'system'"
        >
            {{ $t('configurationTabs.system') }}
        </button>
        <button
            role="tab"
            class="tab"
            :class="{ 'tab-active': activeTab === 'cms' }"
            :aria-selected="activeTab === 'cms'"
            @click="activeTab = 'cms'"
        >
            {{ $t('configurationTabs.cms') }}
        </button>
    </div>

    <div v-show="activeTab === 'system'" class="flex flex-wrap gap-4 py-4">
        <GlobalSettings />
        <ContentLocale />
        <ContentType />
        <MailTargets />
        <NodeTemplates />
        <VideoProfiles />
    </div>

    <div v-show="activeTab === 'cms'" class="flex flex-wrap gap-4 py-4">
        <CmsSettings />
    </div>
</template>
