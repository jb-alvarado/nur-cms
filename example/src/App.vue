<script setup lang="ts">
import { RouterView, useRoute } from 'vue-router'

import HeaderMenu from './components/HeaderMenu.vue'

const route = useRoute()
</script>

<template>
    <div class="min-h-screen">
        <HeaderMenu />
        <main class="overflow-y-auto h-full w-full p-4">
            <RouterView :key="route.fullPath" />
        </main>
    </div>
</template>
