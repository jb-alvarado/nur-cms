/// <reference types="vite/client" />

declare const __FRONTEND_NAME__: string

declare module '*.vue' {
    import type { DefineComponent } from 'vue'

    // biome-ignore lint/complexity/noBannedTypes: reason
    const component: DefineComponent<object, object, any>
    export default component
}
