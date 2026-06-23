<script setup lang="ts">
import HomeArticle from '@/components/HomeArticle.vue'
import HomeFeatures from '@/components/HomeFeatures.vue'
import HomeFooter from '@/components/HomeFooter.vue'
</script>

<template>
    <div class="min-h-screen">
        <!-- Hero Section -->
        <section class="bg-base-200 border-b border-base-content/30">
            <div class="max-w-6xl mx-auto px-4 py-16">
                <div class="text-center">
                    <h1 class="text-5xl font-bold mb-4">NUR CMS Example Frontend</h1>
                    <p class="text-xl text-base-content/70 mb-8">
                        A modern Vue 3 example showcasing how to integrate with the NUR CMS backend API
                    </p>
                    <div class="flex justify-center gap-4">
                        <a href="#entries" class="btn btn-lg btn-primary"> Explore Content </a>
                        <a href="https://github.com/jb-alvarado/nur-cms" target="_blank" class="btn btn-lg btn-dash">
                            View on GitHub
                        </a>
                    </div>
                </div>
            </div>
        </section>

        <!-- Features Section -->
        <section class="max-w-6xl mx-auto px-4 py-16">
            <HomeFeatures />
        </section>

        <!-- Content Section -->
        <section id="entries" class="flex justify-center px-4 pb-16 scroll-mt-20">
            <HomeArticle />
        </section>

        <!-- Info Section -->
        <section class="bg-base-200 border-t border-base-content/30 mt-16">
            <HomeFooter />
        </section>
    </div>
</template>
