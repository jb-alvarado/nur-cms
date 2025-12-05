<script setup lang="ts">
// import { ref, computed } from 'vue'
// import { errMsg } from '@/utils/error'
// import { useAuth } from '@/stores/auth'
// import { useIndex } from '@/stores/index'

import GlobalSettings from '@/components/config/GlobalSettings.vue'
import ContentType from '@/components/config/ContentType.vue'
import MailTargets from '@/components/config/MailTargets.vue';

// const auth = useAuth()
// const store = useIndex()
</script>

<template>
    <div>
        <div class="flex">
            <h1 class="text-2xl">{{ $t('button.configure') }}</h1>
        </div>
    </div>

    <div class="flex flex-wrap gap-4">
        <GlobalSettings />
        <ContentType />
        <MailTargets />
    </div>
</template>
