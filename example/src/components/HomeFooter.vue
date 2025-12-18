<template>
    <div class="max-w-6xl mx-auto px-4 py-16">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-12">
            <div>
                <h3 class="text-2xl font-bold mb-4">About This Example</h3>
                <p class="text-base-content/70 mb-4">
                    This frontend demonstrates how to build a modern Vue 3 application with the NUR CMS backend. It
                    showcases essential patterns like API integration, data fetching, pagination, and error handling.
                </p>
                <p class="text-base-content/70">
                    Use this as a foundation to build your own custom frontends - whether it's a blog, portfolio,
                    documentation site, or any other content-driven application.
                </p>
            </div>

            <div>
                <h3 class="text-2xl font-bold mb-4">Getting Started</h3>
                <ul class="space-y-3">
                    <li class="flex items-start gap-3">
                        <span class="text-primary font-bold">1.</span>
                        <span class="text-base-content/70">Clone or fork the NUR CMS repository</span>
                    </li>
                    <li class="flex items-start gap-3">
                        <span class="text-primary font-bold">2.</span>
                        <span class="text-base-content/70"
                            >Install dependencies with
                            <code class="bg-base-content/10 px-2 py-1 rounded">npm install</code></span
                        >
                    </li>
                    <li class="flex items-start gap-3">
                        <span class="text-primary font-bold">3.</span>
                        <span class="text-base-content/70"
                            >Run the backend: <code class="bg-base-content/10 px-2 py-1 rounded">cargo run</code></span
                        >
                    </li>
                    <li class="flex items-start gap-3">
                        <span class="text-primary font-bold">4.</span>
                        <span class="text-base-content/70"
                            >Start this frontend:
                            <code class="bg-base-content/10 px-2 py-1 rounded">npm run dev:example</code></span
                        >
                    </li>
                </ul>
            </div>
        </div>
    </div>
</template>
