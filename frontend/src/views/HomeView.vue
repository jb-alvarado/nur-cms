<script setup lang="ts">
import { useHead } from '@unhead/vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
useHead({
    title: t('button.home'),
})
</script>

<template>
    <div class="w-full h-full flex items-center justify-center">
        <div class="text-center opacity-30">
            <img src="/logo.svg" class="w-96 min-w-40" alt="Logo" />
            <h1 class="font-bold text-5xl mt-5">NUR CMS</h1>
        </div>
    </div>
</template>
