<script setup lang="ts">
import { useAuth } from '@/stores/auth'

const authStore = useAuth()

const menuItems = [
    { icon: 'bi-card-image', name: 'Media' },
    { icon: 'bi-pencil-square', name: 'Article' },
    { icon: 'bi-file-earmark-text', name: 'Page' },
    { icon: 'bi-card-list', name: 'Blocks' },
]
</script>

<template>
    <div class="w-[200px] h-full bg-base-300 flex flex-col">
        <div class="flex justify-center p-6">
            <div class="join join-vertical w-40">
                <button
                    v-for="item in menuItems"
                    :key="item.name"
                    class="btn join-item p-1 justify-normal items-center"
                >
                    <i class="bi p-1 text-xl leading-0" :class="item.icon"></i>
                    {{ item.name }}
                </button>
            </div>
        </div>
        <div class="grow"></div>
        <div class="flex justify-center p-6">
            <RouterLink to="/user"
                class="btn bg-accent hover:bg-accent/90 text-accent-content w-40 rounded-full p-1 justify-normal items-center"
            >
                <i class="bi bi-person-circle text-3xl leading-0"></i>
                <span class="px-2 truncate">{{ authStore.user.first_name }} {{ authStore.user.last_name }}</span>
            </RouterLink>
        </div>
    </div>
</template>
