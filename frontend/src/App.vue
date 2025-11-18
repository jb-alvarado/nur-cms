<script setup lang="ts">
import { onBeforeMount, onMounted } from 'vue'
import { RouterView, useRoute } from 'vue-router'

import { useAuth } from '@/stores/auth'

import AlertMsg from '@/components/AlertMsg.vue'
import LoginView from '@/views/LoginView.vue'
import MenuSide from '@/components/MenuSide.vue'

const route = useRoute()
const auth = useAuth()

onBeforeMount(async () => {
    await auth.inspectToken()
})

onMounted(() => {
    if (auth.isLogin) {
        auth.selectAuthUser()
        auth.obtainUuid()
    }
})
</script>

<template>
    <div class="h-screen bg-base-300">
        <template v-if="auth.isLogin">
            <div class="flex flex-row h-full">
                <MenuSide class="pt-3" />
                <main class="overflow-y-auto w-full bg-base-100 px-7 pt-3">
                    <RouterView :key="route.fullPath" />
                </main>
            </div>

            <AlertMsg />
        </template>
        <LoginView v-else />
    </div>
</template>
