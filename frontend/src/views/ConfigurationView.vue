<script setup lang="ts">
import GlobalSettings from '@/components/config/GlobalSettings.vue'
import ContentLocale from '@/components/config/ContentLocale.vue'
import ContentType from '@/components/config/ContentType.vue'
import MailTargets from '@/components/config/MailTargets.vue'
import NodeTemplates from '@/components/config/NodeTemplates.vue'

const appVersion = __APP_VERSION__
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl grow">{{ $t('button.configure') }}</h1>

            <div class="text-xs text-base-content/60">v{{ appVersion }}</div>
        </div>
    </div>

    <div class="flex flex-wrap gap-4 py-4">
        <GlobalSettings />
        <ContentLocale />
        <ContentType />
        <MailTargets />
        <NodeTemplates />
    </div>
</template>
