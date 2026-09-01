class NurCmsEcho extends HTMLElement {
    subscriptions = []

    connectedCallback() {
        this.disconnectContext()
        if (this.context) {
            this.subscriptions = [
                this.context.onLocationChange(() => this.render()),
                this.context.onLocaleChange(() => this.render()),
                this.context.onThemeChange(() => this.render()),
            ]
        }
        this.render()
    }

    disconnectedCallback() {
        this.disconnectContext()
    }

    disconnectContext() {
        for (const unsubscribe of this.subscriptions) unsubscribe()
        this.subscriptions = []
    }

    render() {
        this.replaceChildren()

        const locale = this.context?.locale() ?? 'en'
        const theme = this.context?.theme() ?? 'light'
        const location = this.context?.location()
        const german = locale.startsWith('de')
        const adminPage = location?.relativePath.startsWith('/admin-tools') ?? false

        const card = document.createElement('section')
        card.className = 'nur-echo-card'
        card.dataset.theme = theme
        const body = document.createElement('div')
        body.className = 'nur-echo-card-body'

        const title = document.createElement('h1')
        title.className = 'nur-echo-title'
        title.textContent = adminPage
            ? german
                ? 'Admin-Werkzeuge'
                : 'Admin tools'
            : german
              ? 'Echo-Übersicht'
              : 'Echo overview'

        const description = document.createElement('p')
        description.textContent = german
            ? 'Diese Web Component verwendet ausschließlich den vom CMS bereitgestellten Kontext.'
            : 'This web component uses only the context provided by the CMS.'

        const context = document.createElement('dl')
        context.className = 'nur-echo-context'
        this.addContextValue(context, 'Route', location?.relativePath ?? '/')
        this.addContextValue(context, german ? 'Sprache' : 'Locale', locale)
        this.addContextValue(context, 'Theme', theme)
        this.addContextValue(context, german ? 'Rollen' : 'Roles', this.context?.roles().join(', ') ?? '')

        const navigation = document.createElement('nav')
        navigation.className = 'nur-echo-actions'
        navigation.append(this.navigationButton(german ? 'Übersicht' : 'Overview', 'overview'))
        if (this.context?.hasRole('admin')) {
            navigation.append(this.navigationButton(german ? 'Admin-Werkzeuge' : 'Admin tools', 'admin-tools'))
        }

        const button = document.createElement('button')
        button.type = 'button'
        button.className = 'nur-echo-button'
        button.dataset.action = 'request'
        button.textContent = german ? 'Geschützte Route aufrufen' : 'Call protected plugin route'
        button.addEventListener('click', () => this.request())

        const result = document.createElement('pre')
        result.dataset.result = ''
        result.className = 'nur-echo-result'
        result.textContent = german ? 'Noch keine Anfrage ausgeführt.' : 'No request made yet.'

        body.append(title, description, context, navigation, button, result)
        card.append(body)
        this.append(card)
    }

    addContextValue(list, label, value) {
        const term = document.createElement('dt')
        term.textContent = label
        const description = document.createElement('dd')
        description.textContent = value
        list.append(term, description)
    }

    navigationButton(label, path) {
        const button = document.createElement('button')
        button.type = 'button'
        button.className = 'nur-echo-button nur-echo-button-secondary'
        button.textContent = label
        button.addEventListener('click', async () => {
            try {
                await this.context?.navigate(path)
            } catch (error) {
                this.context?.notify('error', error instanceof Error ? error.message : 'Navigation failed.')
            }
        })
        return button
    }

    async request() {
        const result = this.querySelector('[data-result]')
        const button = this.querySelector('[data-action="request"]')
        if (!result) return
        if (!this.context?.request) {
            result.textContent = 'The CMS API context is unavailable.'
            return
        }

        result.textContent = 'Loading…'
        if (button instanceof HTMLButtonElement) button.disabled = true
        try {
            const response = await this.context.request('/api/plugins/echo/editor')
            const body = await response.text()
            if (!response.ok) {
                throw new Error(body || `Request failed with status ${response.status}.`)
            }
            result.textContent = body
        } catch (error) {
            result.textContent = error instanceof Error ? error.message : 'The request failed.'
        } finally {
            if (button instanceof HTMLButtonElement) button.disabled = false
        }
    }
}

customElements.define('nur-cms-echo', NurCmsEcho)
