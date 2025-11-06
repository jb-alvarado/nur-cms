import { defineStore } from 'pinia'
import { useAuth } from '@/stores/auth'

export const useIndex = defineStore('index', {
    state: () => ({
        darkMode: false,
        msgList: [] as alertMessage[],
        contentType: { 'content-type': 'application/json;charset=UTF-8' },
        limit: localStorage.getItem('limit') ?? 10,
        limits: [2, 10, 25, 50, 100],
        ordering: 'id',
        next: null as null | string,
        previous: null as null | string,
        allRows: [
            { check: true, active: true, up: true, name: 'ID', field: 'id' },
            { check: false, active: false, up: false, name: 'Title', field: 'title' },
            { check: false, active: false, up: false, name: 'Slug', field: 'slug' },
            { check: false, active: false, up: false, name: 'Status', field: 'status' },
            { check: false, active: false, up: false, name: 'Author', field: 'author' },
            { check: false, active: false, up: false, name: 'Locale', field: 'locale' },
            { check: false, active: false, up: false, name: 'Created At', field: 'created_at' },
            { check: false, active: false, up: false, name: 'Updated At', field: 'updated_at' },
        ],
        visibleRows: [
            { active: true, up: true, name: 'ID', field: 'id' },
            { active: false, up: false, name: 'Title', field: 'title' },
            { active: false, up: false, name: 'Status', field: 'status' },
            { active: false, up: false, name: 'Created At', field: 'created_at' },
        ],
        search: '',
        tableCols: [] as Content[],
        types: [] as ContentTypeExt[],
        type: 'article',
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

        async selectTypes() {
            await fetch('/api/content/types')
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = (await resp.json())?.error ?? (await resp.text())
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results?.length > 0) {
                        this.types = response.results
                    }
                })
                .catch((e) => {
                    this.msgAlert('error', e, 6)
                })
        },

        initContent(type: string) {
            this.type = type
            const visibleFields = localStorage.getItem('visibleFields')

            if (visibleFields) {
                this.visibleRows = JSON.parse(visibleFields)
            }

            const visibleSet = new Set(this.visibleRows.map((r: any) => r.field))
            for (const r of this.allRows) {
                r.check = visibleSet.has(r.field)
            }
        },

        activeFields() {
            this.visibleRows = this.allRows
                .filter((r) => r.check)
                .map((r) => ({ active: r.active, up: r.up, name: r.name, field: r.field }))

            localStorage.setItem('visibleFields', JSON.stringify(this.visibleRows))
            this.contentSelect()
        },

        setItemLimit() {
            localStorage.setItem('limit', this.limit.toString())
            this.contentSelect()
        },

        async searchItem() {
            if (this.search.length > 2) {
                await this.contentSelect(this.search)
            } else if (this.search.length === 0) {
                await this.contentSelect()
            }
        },

        async contentSelect(sr: string | null = null, u: string | null = null) {
            const fields = this.visibleRows.map((r: any) => r.field).join(',')
            const auth = useAuth()

            const url = u
                ? u
                : sr
                  ? `/api/content/entries/${this.type}?fields=${fields}&limit=${this.limit}&ordering=${this.ordering}&search=${sr}`
                  : `/api/content/entries/${this.type}?fields=${fields}&limit=${this.limit}&ordering=${this.ordering}`

            await fetch(url, {
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = (await resp.json())?.error ?? (await resp.text())
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results?.length > 0) {
                        this.next = response.next
                        this.previous = response.previous
                        this.tableCols = response.results.map((o: any) => ({ check: false, ...o }))
                    } else {
                        this.tableCols = []
                    }
                })
                .catch((e) => {
                    this.msgAlert('error', e, 6)
                })
        },

        async updateStatus(status: string) {
            const auth = useAuth()

            for (const item of this.tableCols) {
                if (item.check) {
                    await fetch(`/api/content/entries/${item.id}`, {
                        method: 'PUT',
                        headers: { ...this.contentType, ...auth.authHeader },
                        body: JSON.stringify({ status }),
                    })
                        .then(async (resp) => {
                            const text = await resp.text()
                            let msg: string

                            if (resp.status >= 400) {
                                try {
                                    const json = JSON.parse(text)
                                    msg = json?.error ?? (typeof json === 'string' ? json : JSON.stringify(json))
                                } catch {
                                    msg = text
                                }
                                this.msgAlert('error', msg, 6)
                            } else {
                                this.msgAlert('success', `Update: ${item.title ?? item.id}`, 2)
                            }
                        })
                        .catch((e) => {
                            this.msgAlert('error', e, 6)
                        })
                }
            }

            await this.contentSelect()
        },

        async contentDelete() {
            const auth = useAuth()

            for (const item of this.tableCols) {
                if (item.check) {
                    await fetch(`/api/content/entries/${item.id}`, {
                        method: 'DELETE',
                        headers: auth.authHeader,
                    })
                        .then(async (resp) => {
                            const text = await resp.text()
                            let msg: string

                            if (resp.status >= 400) {
                                try {
                                    const json = JSON.parse(text)
                                    msg = json?.error ?? (typeof json === 'string' ? json : JSON.stringify(json))
                                } catch {
                                    msg = text
                                }

                                if (!msg) {
                                    msg = resp.statusText
                                }
                                this.msgAlert('error', msg, 6)
                            } else {
                                this.msgAlert('success', `Deleted: ${item.title ?? item.id}`, 2)
                            }
                        })
                        .catch((e) => {
                            this.msgAlert('error', e, 6)
                        })
                }
            }

            await this.contentSelect()
        },
    },
})
