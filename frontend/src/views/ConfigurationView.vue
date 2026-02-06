<script setup lang="ts">
import { version } from '../../../package.json'

import GlobalSettings from '@/components/config/GlobalSettings.vue'
import ContentType from '@/components/config/ContentType.vue'
import MailTargets from '@/components/config/MailTargets.vue'
import NodeTemplates from '@/components/config/NodeTemplates.vue'
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ $t('button.configure') }}</h1>

            <div class="text-xs text-base-content/60">v{{ version }}</div>
        </div>
    </div>

    <div class="flex flex-wrap gap-4 py-4">
        <GlobalSettings />
        <ContentType />
        <MailTargets />
        <NodeTemplates />
    </div>
</template>
