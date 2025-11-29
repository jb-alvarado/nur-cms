import { createI18n } from 'vue-i18n'

import deDE from './locales/de-DE.ts'
import enUS from './locales/en-US.ts'

export const locales = [
    {
        code: 'de',
        language: 'de-DE',
        name: 'Deutsch',
    },
    {
        code: 'en',
        language: 'en-US',
        name: 'English',
    },
]

const instance = createI18n({
    legacy: false,
    locale: 'en-US',
    messages: {
        de: deDE,
        en: enUS,
    },
})

export default instance

export const i18n = instance.global
