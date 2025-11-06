import { defineStore } from 'pinia'
import { jwtDecode } from 'jwt-decode'
import { useIndex } from '@/stores/index'

export const useAuth = defineStore('auth', {
    state: () => ({
        isLogin: false,
        jwtToken: '',
        jwtRefresh: '',
        authHeader: {},
        id: 0,
        role: 'guest' as Role,
        user: {} as AuthUser,
        lastLogin: null as string | null | undefined,
    }),

    getters: {},
    actions: {
        updateToken(token: string, refresh: string) {
            const decodedToken = jwtDecode<JwtPayloadExt>(token)

            localStorage.setItem('token', token)
            localStorage.setItem('refresh', refresh)

            this.isLogin = true
            this.jwtToken = token
            this.jwtRefresh = refresh
            this.authHeader = { Authorization: `Bearer ${token}` }
            this.id = decodedToken.id
            this.role = decodedToken.role
        },

        removeToken() {
            localStorage.removeItem('token')
            localStorage.removeItem('refresh')

            this.isLogin = false
            this.jwtToken = ''
            this.jwtRefresh = ''
            this.authHeader = {}
            this.id = 0
            this.role = 'guest'
            this.user = {}
        },

        async obtainToken(username: string, password: string) {
            let code = 400
            const payload = {
                username,
                password,
            }

            await fetch('/auth/login', {
                method: 'POST',
                headers: new Headers([['content-type', 'application/json;charset=UTF-8']]),
                body: JSON.stringify(payload),
            })
                .then((resp) => {
                    code = resp.status

                    if (code === 200) {
                        return resp.json()
                    }
                })
                .then((response: Token) => {
                    if (response?.access) {
                        this.updateToken(response.access, response.refresh)
                    }
                })
                .catch((e) => {
                    code = typeof e.status === 'number' ? e.status : code
                })

            return code
        },

        async refreshToken() {
            await fetch('/auth/refresh', {
                method: 'POST',
                headers: new Headers([['content-type', 'application/json;charset=UTF-8']]),
                body: JSON.stringify({ refresh: this.jwtRefresh }),
            })
                .then((resp) => resp.json())
                .then((response: any) => {
                    if (response.access) {
                        this.updateToken(response.access, this.jwtRefresh)
                        this.isLogin = true
                    }
                })
                .catch(() => {
                    this.removeToken()
                })
        },

        async inspectToken() {
            const token = localStorage.getItem('token')
            const refresh = localStorage.getItem('refresh')

            if (token && refresh) {
                const decodedToken = jwtDecode<JwtPayloadExt>(token)
                const decodedRefresh = jwtDecode<JwtPayloadExt>(refresh)
                const timestamp = Date.now() / 1000
                const expireToken = decodedToken.exp
                const expireRefresh = decodedRefresh.exp || 0

                if (expireToken && expireToken - timestamp > 15) {
                    this.isLogin = true
                    this.jwtToken = token
                    this.jwtRefresh = refresh
                    this.authHeader = { Authorization: `Bearer ${token}` }
                    this.id = decodedToken.id
                    this.role = decodedToken.role
                } else if (expireRefresh && expireRefresh - timestamp > 0) {
                    await this.refreshToken()
                } else {
                    // Prompt user to re-login.
                    this.removeToken()
                }
            } else {
                this.removeToken()
            }
        },

        async selectAuthUser() {
            const store = useIndex()
            await fetch('/api/auth-user', {
                headers: this.authHeader,
            })
                .then(async (resp) => {
                    if (resp.status >= 400) {
                        const msg = (await resp.json())?.error ?? (await resp.text())

                        if (msg.includes('Unauthorized')) {
                            this.removeToken()
                        }
                        throw new Error(msg)
                    }
                    return resp.json()
                })
                .then((response: RespondObj) => {
                    if (response.results.length > 0) {
                        this.user = response.results[0]
                        this.lastLogin = this.user.last_login
                        delete this.user.id
                        delete this.user.last_login
                        delete this.user.role
                    }
                })
                .catch((e) => {
                    store.msgAlert('error', e, 3)
                })
        },
    },
})
