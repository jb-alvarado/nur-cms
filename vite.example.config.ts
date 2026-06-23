import { fileURLToPath, URL } from 'node:url'

import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
    build: {
        chunkSizeWarningLimit: 1600,
        outDir: './dist',
    },
    base: '/',
    root: './example',
    plugins: [tailwindcss(), vue()],
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./example/src', import.meta.url)),
        },
    },
    server: {
        host: '127.0.0.1',
        port: 5758,
        proxy: {
            '/api': { target: 'http://127.0.0.1:8777', changeOrigin: true },
            '/auth': { target: 'http://127.0.0.1:8777', changeOrigin: true },
            '/sse': { target: 'http://127.0.0.1:8777', changeOrigin: true },
            '/uploads': { target: 'http://127.0.0.1:8777', changeOrigin: true },
        },
    },
})
