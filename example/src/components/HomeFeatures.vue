<script setup lang="ts">
const features = [
    {
        id: 0,
        bgColor: 'bg-blue-100/10',
        textColor: 'text-blue-600',
        icon: 'bi-lightning-charge',
        title: 'Fast & Efficient',
        text: 'Built with Rust and Axum, the backend delivers lightning-fast API responses optimized for performance.',
    },
    {
        id: 1,
        bgColor: 'bg-green-100/10',
        textColor: 'text-green-600',
        icon: 'bi-check-circle',
        title: 'Flexible Content',
        text: 'Output content in multiple formats: Markdown, HTML, or AST for complete control over rendering.',
    },
    {
        id: 1,
        bgColor: 'bg-purple-100/10',
        textColor: 'text-purple-600',
        icon: 'bi-sliders',
        title: 'Fully Customizable',
        text: 'RESTful API makes it easy to build custom frontends with any framework you prefer.',
    },
]
</script>
<template>
    <h2 class="text-3xl font-bold mb-12 text-center">Key Features</h2>
    <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
        <div
            v-for="feature in features"
            :key="feature.id"
            class="bg-base-200 rounded-lg shadow-md p-8 hover:shadow-lg transition"
        >
            <div class="w-12 h-12 rounded-lg flex items-center justify-center mb-4" :class="feature.bgColor">
                <i class="bi text-2xl" :class="`${feature.icon} ${feature.textColor}`"></i>
            </div>
            <h3 class="text-xl font-semibold text-base-content/80 mb-2">{{ feature.title }}</h3>
            <p class="text-base-content/60">
                {{ feature.text }}
            </p>
        </div>
    </div>
</template>
