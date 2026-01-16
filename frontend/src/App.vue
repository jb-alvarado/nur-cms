<script setup lang="ts">
import { onBeforeMount } from 'vue'
import { RouterView, useRoute } from 'vue-router'

import { useAuth } from '@/stores/auth'
import { useIndex } from '@/stores/index'

import AlertMsg from '@/components/AlertMsg.vue'
import LoginView from '@/views/LoginView.vue'
import MenuSide from '@/components/MenuSide.vue'

const route = useRoute()
const auth = useAuth()
const store = useIndex()

onBeforeMount(async () => {
    await auth.inspectToken()
})
</script>

<template>
    <div class="h-screen bg-base-100">
        <template v-if="auth.isLogin || route.name === 'verification'">
            <div class="flex flex-row h-full">
                <MenuSide v-if="auth.isLogin" class="pt-3" />
                <main class="overflow-y-auto w-full bg-base-100 px-7">
                    <RouterView :key="route.fullPath + store.randomKey" />
                </main>
            </div>

            <AlertMsg />
        </template>
        <LoginView v-else />
    </div>
</template>
