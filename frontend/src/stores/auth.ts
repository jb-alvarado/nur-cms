import { defineStore } from 'pinia'
import { jwtDecode } from 'jwt-decode'
import { useIndex } from '@/stores/index'
import { authFetch } from '@/composables/authFetch'

type AuthChannelMessage = { type: 'tokens-updated'; access: string; refresh: string } | { type: 'logout' }

const AUTH_NAMESPACE = 'nur-cms'
const AUTH_CHANNEL_NAME = `${AUTH_NAMESPACE}:auth`
const TOKEN_REFRESH_LOCK = `${AUTH_NAMESPACE}:token-refresh`

let refreshRequest: Promise<boolean> | null = null
let authChannel: BroadcastChannel | null = null

function isTokenPair(access: string, refresh: string): boolean {
    const decodedAccess = jwtDecode<JwtPayloadExt>(access)
    const decodedRefresh = jwtDecode<JwtPayloadExt>(refresh)

    return decodedAccess.token_type === 'access' && decodedRefresh.token_type === 'refresh'
}

function broadcast(message: AuthChannelMessage) {
    authChannel?.postMessage(message)
}

export function initAuthChannel() {
    if (authChannel || typeof BroadcastChannel === 'undefined') return

    authChannel = new BroadcastChannel(AUTH_CHANNEL_NAME)
    authChannel.addEventListener('message', (event: MessageEvent<AuthChannelMessage>) => {
        const message = event.data
        if (!message || typeof message !== 'object') return

        const auth = useAuth()
        if (message.type === 'logout') {
            auth.removeToken(false)
            return
        }

        if (message.type === 'tokens-updated') {
            try {
                auth.updateToken(message.access, message.refresh, false)
            } catch {
                auth.removeToken(false)
            }
        }
    })
}

export const useAuth = defineStore('auth', {
    state: () => ({
        isLogin: false,
        verificationPending: false,
        jwtToken: '',
        jwtRefresh: '',
        authHeader: {},
        id: 0,
        role: 'guest' as Role,
        username: '',
        user: {} as AuthUser,
        lastLogin: null as string | null | undefined,
        uuid: null as null | string,
    }),

    getters: {},
    actions: {
        updateToken(token: string, refresh: string, shouldBroadcast = true) {
            if (!isTokenPair(token, refresh)) {
                throw new Error('Invalid token types')
            }
            const decodedToken = jwtDecode<JwtPayloadExt>(token)

            localStorage.setItem('token', token)
            localStorage.setItem('refresh', refresh)

            this.isLogin = true
            this.verificationPending = false
            this.jwtToken = token
            this.jwtRefresh = refresh
            this.authHeader = { Authorization: `Bearer ${token}` }
            this.id = decodedToken.id
            this.role = decodedToken.role

            if (shouldBroadcast) {
                broadcast({ type: 'tokens-updated', access: token, refresh })
            }
        },

        removeToken(shouldBroadcast = true) {
            localStorage.removeItem('token')
            localStorage.removeItem('refresh')

            this.isLogin = false
            this.jwtToken = ''
            this.jwtRefresh = ''
            this.authHeader = {}
            this.id = 0
            this.role = 'guest'
            this.user = {}
            this.uuid = null
            this.verificationPending = false
            useIndex().resetPlugins()

            if (shouldBroadcast) {
                broadcast({ type: 'logout' })
            }
        },

        async obtainVerificationCode(password: string) {
            let code = 400

            const payload = {
                username: this.username,
                password,
            }

            try {
                const response = await fetch('/auth/login', {
                    method: 'POST',
                    headers: new Headers([['content-type', 'application/json;charset=UTF-8']]),
                    body: JSON.stringify(payload),
                })
                code = response.status
                const data = (await response.json()) as Partial<Token>
                if (!response.ok) return code

                if (data.access && data.refresh) {
                    this.updateToken(data.access, data.refresh)
                } else {
                    this.verificationPending = true
                }
            } catch {
                this.verificationPending = false
            }

            return code
        },

        async verifyCode(verificationCode: string) {
            let code = 400

            const payload = {
                username: this.username,
                code: verificationCode,
            }

            try {
                const response = await fetch('/auth/verify', {
                    method: 'POST',
                    headers: new Headers([['content-type', 'application/json;charset=UTF-8']]),
                    body: JSON.stringify(payload),
                })
                code = response.status
                if (!response.ok) return code

                const data = (await response.json()) as Partial<Token>
                if (!data.access || !data.refresh) return 400
                this.updateToken(data.access, data.refresh)
            } catch {
                return code
            }

            return code
        },

        async refreshToken(): Promise<boolean> {
            if (refreshRequest) return refreshRequest

            const refreshBeforeLock = this.jwtRefresh
            refreshRequest = this.withRefreshLock(async () => {
                const storedRefresh = localStorage.getItem('refresh')
                const storedToken = localStorage.getItem('token')

                // Another tab completed the rotation before this tab got the lock.
                if (storedRefresh && storedToken && storedRefresh !== refreshBeforeLock) {
                    this.updateToken(storedToken, storedRefresh, false)
                    return true
                }

                try {
                    const response = await fetch('/auth/refresh', {
                        method: 'POST',
                        headers: new Headers([['content-type', 'application/json;charset=UTF-8']]),
                        body: JSON.stringify({ refresh: refreshBeforeLock }),
                    })
                    if (!response.ok) {
                        this.removeToken()
                        return false
                    }

                    const data = (await response.json()) as Partial<Token>
                    if (!data.access || !data.refresh) {
                        this.removeToken()
                        return false
                    }

                    this.updateToken(data.access, data.refresh)
                    return true
                } catch {
                    this.removeToken()
                    return false
                }
            })

            try {
                return await refreshRequest
            } finally {
                refreshRequest = null
            }
        },

        async withRefreshLock(task: () => Promise<boolean>): Promise<boolean> {
            if (typeof navigator === 'undefined' || !navigator.locks) {
                return task()
            }

            return navigator.locks.request(TOKEN_REFRESH_LOCK, task)
        },

        async logout() {
            const refresh = this.jwtRefresh || localStorage.getItem('refresh') || ''
            this.removeToken()

            if (!refresh) return

            await fetch('/auth/logout', {
                method: 'POST',
                headers: new Headers([['content-type', 'application/json;charset=UTF-8']]),
                body: JSON.stringify({ refresh }),
            }).catch(() => undefined)
        },

        async obtainUuid() {
            try {
                const response = await authFetch<{ uuid: string }>('/sse/generate-uuid', {
                    method: 'POST',
                })
                this.uuid = response.uuid
            } catch {
                this.uuid = null
            }
        },

        async inspectToken() {
            const token = localStorage.getItem('token')
            const refresh = localStorage.getItem('refresh')

            if (token && refresh) {
                try {
                    const decodedToken = jwtDecode<JwtPayloadExt>(token)
                    const decodedRefresh = jwtDecode<JwtPayloadExt>(refresh)
                    if (decodedToken.token_type !== 'access' || decodedRefresh.token_type !== 'refresh') {
                        this.removeToken()
                        return
                    }
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
                } catch {
                    this.removeToken()
                }
            } else {
                this.removeToken()
            }
        },

        async selectAuthUser() {
            const store = useIndex()
            await authFetch<RespondObj>(`/api/auth-user?id=${this.id}`)
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
                    store.msgAlert('error', e)
                })
        },
    },
})
