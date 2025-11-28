<script setup lang="ts">
import { useHead } from '@unhead/vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
useHead({
    title: t('button.home'),
})
</script>

<template>
    <div class="w-full h-full">
        <div>
            {{ $t('home.welcome') }}
        </div>
    </div>
</template>
