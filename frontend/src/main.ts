import './assets/css/main.css'

import { createApp } from 'vue'
import { createHead } from '@unhead/vue/client'
import { createPinia } from 'pinia'
import i18nInstance from './i18n.ts'

import App from './App.vue'
import router from './router'

import { useIndex } from '@/stores/index'

const app = createApp(App)

const preferDark = window.matchMedia('(prefers-color-scheme: dark)')?.matches ?? false
const local = localStorage.getItem('language') || 'en'
const theme = localStorage.getItem('theme') || (preferDark ? 'dark' : 'light')

const head = createHead({
    init: [
        {
            title: 'NUR CMS',
            titleTemplate: '%s | NUR CMS',
            htmlAttrs: { lang: local, 'data-theme': theme },
        },
    ],
})

app.use(i18nInstance)
app.use(head)
app.use(createPinia())
app.use(router)

const indexStore = useIndex()

indexStore.darkMode = (theme === 'dark')

app.mount('#app')
