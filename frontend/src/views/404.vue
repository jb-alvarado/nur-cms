<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<template>
    <div class="h-full w-full flex justify-center items-center">
        <div class="text-center">
            <h1 class="text-9xl font-bold">404</h1>
            <p class="text-3xl mt-6">{{ t('error.notFound') }}!</p>
            <RouterLink class="btn btn-ghost mt-6" to="/">{{ t('button.home') }}</RouterLink>
        </div>
    </div>
</template>
