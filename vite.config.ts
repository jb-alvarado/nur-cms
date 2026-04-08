import { fileURLToPath, URL } from 'node:url'

import { defineConfig, loadEnv } from 'vite'
import vue from '@vitejs/plugin-vue'
// import vueDevTools from 'vite-plugin-vue-devtools'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
    const env = loadEnv(mode, process.cwd(), '')

    return {
        build: {
            chunkSizeWarningLimit: 1600,
        },
        base: '/admin/',
        root: './frontend',
        plugins: [
            tailwindcss(),
            vue(),
            {
                name: 'frontend-name-html',
                transformIndexHtml(html: string) {
                    return html.replace('__FRONTEND_NAME__', env.FRONTEND_NAME || 'NUR CMS')
                },
            },
        ],
        define: {
            __FRONTEND_NAME__: JSON.stringify(env.FRONTEND_NAME || 'NUR CMS'),
        },
        resolve: {
            alias: {
                '@': fileURLToPath(new URL('./frontend/src', import.meta.url)),
            },
        },
        server: {
            host: '127.0.0.1',
            port: 5757,
            proxy: {
                '/api': { target: 'http://127.0.0.1:8777', changeOrigin: true },
                '/auth': { target: 'http://127.0.0.1:8777', changeOrigin: true },
                '/sse': { target: 'http://127.0.0.1:8777', changeOrigin: true },
                '/uploads': { target: 'http://127.0.0.1:8777', changeOrigin: true },
            },
        },
    }
})
