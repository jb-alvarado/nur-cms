import { nextTick } from 'vue'
import { defineStore } from 'pinia'
import { useAuth } from '@/stores/auth'
import { errMsg } from '@/utils/error'

export const useIndex = defineStore('index', {
    state: () => ({
        darkMode: false,
        msgList: [] as alertMessage[],
        contentType: { 'content-type': 'application/json;charset=UTF-8' },
        preview: false,
        limit: Number(localStorage.getItem('limit') ?? 10),
        limits: [10, 25, 50, 100],
        offset: 0,
        total: 0,
        ordering: 'id',
        next: null as null | string,
        previous: null as null | string,
        allRows: [
            { check: true, active: true, up: true, name: 'ID', field: 'id' },
            { check: false, active: false, up: false, name: 'Title', field: 'title' },
            { check: false, active: false, up: false, name: 'Slug', field: 'slug' },
            { check: false, active: false, up: false, name: 'Status', field: 'status' },
            { check: false, active: false, up: false, name: 'Author', field: 'author.first_name,author.last_name' },
            { check: false, active: false, up: false, name: 'Language', field: 'locale_id' },
            { check: false, active: false, up: false, name: 'Start Time', field: 'start_time' },
            { check: false, active: false, up: false, name: 'End Time', field: 'end_time' },
            { check: false, active: false, up: false, name: 'Created At', field: 'created_at' },
            { check: false, active: false, up: false, name: 'Updated At', field: 'updated_at' },
            { check: false, active: false, up: false, name: 'Group ID', field: 'group_id' },
        ] as TableRow[],
        visibleRows: [
            { active: true, up: true, name: 'ID', field: 'id' },
            { active: false, up: false, name: 'Title', field: 'title' },
            { active: false, up: false, name: 'Status', field: 'status' },
            { active: false, up: false, name: 'Created At', field: 'created_at' },
            { active: false, up: false, name: 'Language', field: 'locale_id' },
            { active: false, up: false, name: 'Group ID', field: 'group_id' },
        ] as TableRow[],
        suffix: 'content/entries',
        search: '',
        tableCols: [] as Content[],
        authors: [] as ContentAuthor[],
        locales: [] as Locale[],
        locale: 'en',
        types: [] as ContentTypeExt[],
        type: {} as ContentTypeExt,
        routeType: '',
        progress: 0,
        progressShow: false,
        randomKey: 'aHcyWqp',
        loaded: false,
        selectAll: false,
        isLoaded: false,
    }),

    getters: {},
    actions: {
        msgAlert(variance: string, text: string) {
            const seconds = variance === 'error' ? 6 : variance === 'warning' ? 5 : 3
            const msg = { text, variance, seconds }

            this.msgList.push(msg)

            setTimeout(() => {
                const index = this.msgList.indexOf(msg)
                if (index >= 0) {
                    this.msgList.splice(index, 1)
                }
            }, seconds * 1000)
        },

        async selectAuthors() {
            await fetch('/api/content/authors?fields=id,first_name,last_name,slug&limit=1000')
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results?.length > 0) {
                        this.authors = response.results
                    }
                })
                .catch((e) => {
                    this.msgAlert('error', e)
                })
        },

        async selectLocales() {
            await fetch('/api/locales')
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results?.length > 0) {
                        this.locales = response.results
                    }
                })
                .catch((e) => {
                    this.msgAlert('error', e)
                })
        },

        async selectTypes() {
            await fetch('/api/content/types?ordering=order_index,id')
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results?.length > 0) {
                        this.types = response.results.map((t: ContentTypeExt) => {
                            switch (t.slug) {
                                case 'article':
                                    t.icon = 'bi-card-list'
                                    break
                                case 'collection':
                                    t.icon = 'bi-collection'
                                    break
                                case 'data':
                                    t.icon = 'bi-collection'
                                    break
                                case 'page':
                                    t.icon = 'bi-card-heading'
                                    break
                                default:
                                    if (t.use_meta) {
                                        t.icon = 'bi-calendar-event'
                                    } else {
                                        t.icon = 'bi-card-text'
                                    }
                            }
                            return t
                        })

                        this.loaded = true
                    }
                })
                .catch((e) => {
                    this.msgAlert('error', e)
                })
        },

        saveTableState() {
            localStorage.setItem(`visibleFields:${this.routeType}`, JSON.stringify(this.visibleRows))
        },

        initContent(suffix: string, fromStorage = true) {
            this.suffix = suffix
            const visibleFields = localStorage.getItem(`visibleFields:${this.routeType}`)

            if (fromStorage && visibleFields) {
                this.visibleRows = JSON.parse(visibleFields)
            }

            const visibleSet = new Set(this.visibleRows.map((r: any) => r.field))
            for (const r of this.allRows) {
                r.check = visibleSet.has(r.field)
            }

            for (const row of this.visibleRows) {
                if (row.active && row.up) {
                    this.ordering = row.field
                } else if (row.active && !row.up) {
                    this.ordering = `-${row.field}`
                }
            }
        },

        activeFields() {
            const existingRows = new Map(this.visibleRows.map((row) => [row.field, row]))

            this.visibleRows = this.allRows
                .filter((r) => r.check)
                .map((r) => {
                    const existingRow = existingRows.get(r.field)

                    return {
                        active: existingRow?.active ?? r.active,
                        up: existingRow?.up ?? r.up,
                        name: r.name,
                        field: r.field,
                    }
                })

            this.saveTableState()
            this.contentSelect()
        },

        setItemLimit() {
            localStorage.setItem('limit', this.limit.toString())
            this.contentSelect()
        },

        async searchItem() {
            nextTick(async () => {
                await this.contentSelect()
            })
        },

        async contentSelect(u: string | null = null) {
            const auth = useAuth()
            const fields = this.visibleRows
                .map((r: any) => r.field)
                .map((field: string) => {
                    if (field === 'start_time' || field === 'end_time') {
                        return 'meta'
                    }
                    return field
                })
                .filter((field: string, index: number, arr: string[]) => arr.indexOf(field) === index)
                .join(',')

            let type = ''
            let offsetVar = ''

            if (this.suffix === 'content/entries') {
                type = `type_slug=${this.routeType}&`
            }

            if (this.offset > 0) {
                offsetVar = `&offset=${this.offset}`
            }

            let url = u
                ? u
                : `/api/${this.suffix}?${type}grouped=true&locale=${this.locale}&fields=${fields},group_members&limit=${this.limit}${offsetVar}&ordering=${this.ordering}`

            if (this.search) {
                url = `${url}&search=${this.search}`
            }

            await fetch(url, {
                headers: auth.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = await errMsg(resp)
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results?.length > 0) {
                        this.next = response.next
                        this.previous = response.previous
                        this.total = response.count
                        this.tableCols = response.results.map((o: any) => ({ check: false, ...o }))
                    } else {
                        this.tableCols = []
                    }
                })
                .catch((e) => {
                    this.msgAlert('error', e)
                })
        },

        async updateStatus(status: string, updateGroup: boolean = false) {
            const auth = useAuth()

            for (const item of this.tableCols) {
                if (item.check) {
                    let ids = [item.id!]
                    if (updateGroup) {
                        ids = item.group_members?.map((i) => i.id) ?? [item.id!]
                    }

                    for (const id of ids) {
                        await fetch(`/api/${this.suffix}/${id}`, {
                            method: 'PUT',
                            headers: { ...this.contentType, ...auth.authHeader },
                            body: JSON.stringify({ status }),
                        })
                            .then(async (resp) => {
                                if (resp.status >= 400) {
                                    const msg = await errMsg(resp)
                                    throw new Error(msg)
                                } else {
                                    this.msgAlert('success', `Update: ${item.title ?? id}`)
                                }
                            })
                            .catch((e) => {
                                this.msgAlert('error', e)
                            })
                    }
                }
            }

            await this.contentSelect()
        },

        async contentDelete(deleteGroup: boolean = false) {
            const auth = useAuth()

            for (const item of this.tableCols) {
                if (item.check) {
                    let ids = [item.id!]
                    if (deleteGroup) {
                        ids = item.group_members?.map((i) => i.id) ?? [item.id!]
                    }

                    for (const id of ids) {
                        await fetch(`/api/${this.suffix}/${id}`, {
                            method: 'DELETE',
                            headers: auth.authHeader,
                        })
                            .then(async (resp) => {
                                if (resp.status >= 400) {
                                    const msg = await errMsg(resp)
                                    throw new Error(msg)
                                } else {
                                    this.msgAlert('success', `Deleted: ${item.title ?? id}`)
                                }
                            })
                            .catch((e) => {
                                this.msgAlert('error', e)
                            })
                    }
                }
            }

            await this.contentSelect()
            this.selectAll = false
        },
    },
})
