import { defineStore } from 'pinia'

export const useIndex = defineStore('index', {
    state: () => ({
        darkMode: false,
        msgList: [] as alertMessage[],
        contentType: { 'content-type': 'application/json;charset=UTF-8' },
    }),

    getters: {},
    actions: {
        msgAlert(variance: string, text: string, seconds: number = 3) {
            const msg = { text, variance, seconds }

            this.msgList.push(msg)

            setTimeout(() => {
                const index = this.msgList.indexOf(msg)
                if (index >= 0) {
                    this.msgList.splice(index, 1)
                }
            }, seconds * 1000)
        },
    },
})
