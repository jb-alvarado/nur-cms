import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
    define: {
        'process.env.NODE_ENV': JSON.stringify('production'),
    },
    plugins: [
        vue({
            features: {
                customElement: /\.ce\.vue$/,
            },
        }),
    ],
    build: {
        target: 'es2022',
        lib: {
            entry: 'src/index.ts',
            formats: ['es'],
            fileName: () => 'index.js',
        },
        rollupOptions: {
            output: {
                codeSplitting: false,
            },
        },
    },
})

// TODO: Evaluate sharing the host's Vue runtime once a stable, versioned runtime contract exists.
// For this proof of concept Vue stays bundled so the plugin remains independently deployable.
