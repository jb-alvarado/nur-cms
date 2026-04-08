import './assets/css/main.css'

import { createApp } from 'vue'
import { createHead } from '@unhead/vue/client'
import { createPinia } from 'pinia'
import i18nInstance from './i18n.ts'

import App from './App.vue'
import router from './router'

const frontendName = __FRONTEND_NAME__
const app = createApp(App)

const head = createHead({
    init: [
        {
            title: frontendName,
            titleTemplate: `%s | ${frontendName}`,
            htmlAttrs: { lang: 'en' },
        },
    ],
})

document.title = frontendName

app.use(i18nInstance)
app.use(head)
app.use(createPinia())
app.use(router)

router.isReady().then(() => {
    app.mount('#app')
})
