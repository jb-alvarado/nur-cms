<script setup lang="ts">
import { onBeforeMount, onMounted } from 'vue'
import { RouterView } from 'vue-router'

import { useAuth } from '@/stores/auth'

import AlertMsg from '@/components/AlertMsg.vue'
import LoginView from '@/views/LoginView.vue'
// import MenuHeader from '@/components/MenuHeader.vue'
import MenuSide from '@/components/MenuSide.vue'

const authStore = useAuth()

onBeforeMount(async () => {
    await authStore.inspectToken()
})

onMounted(() => {
    if (authStore.isLogin) {
        authStore.selectAuthUser()
    }
})
</script>

<template>
    <div class="h-screen bg-base-300">
        <template v-if="authStore.isLogin">
            <div class="flex flex-row h-full">
                <MenuSide class="pt-3" />
                <main class="overflow-y-auto w-full bg-base-100 px-7 pt-3">
                    <RouterView />
                </main>
            </div>

            <AlertMsg />
        </template>
        <LoginView v-else />
    </div>
</template>
